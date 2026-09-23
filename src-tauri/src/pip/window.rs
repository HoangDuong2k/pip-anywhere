//! The floating PiP window: borderless, always on top, drag to move, drag edges to resize,
//! Esc / middle-click / × to close. Opacity is set from the control panel.

use std::io::BufRead;
use std::num::NonZeroU32;
use std::rc::Rc;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, ResizeDirection, Window, WindowId, WindowLevel};

use super::render;
use super::{ChildMessage, Command, Geometry, PipConfig, MIN_OPACITY};
use crate::capture::{self, CaptureEvent, CaptureHandle, CaptureRequest, Frame, FrameSink};

/// Width of the invisible band along the edges that starts a resize.
const RESIZE_BORDER: f64 = 8.0;

#[derive(Debug)]
enum UserEvent {
    Capture(CaptureEvent),
    CloseRequested,
    SetOpacity(f32),
}

type Surface = softbuffer::Surface<Rc<Window>, Rc<Window>>;

struct PipApp {
    config: PipConfig,
    proxy: EventLoopProxy<UserEvent>,
    sink: FrameSink,
    capture: Option<CaptureHandle>,
    window: Option<Rc<Window>>,
    surface: Option<Surface>,
    frame: Option<Frame>,
    cursor: PhysicalPosition<f64>,
    hovered: bool,
    opacity: f32,
    started: bool,
    closing: bool,
}

pub fn run(config: PipConfig) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event().build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();

    // Control app -> child commands. EOF means the control app is gone: close too.
    let stdin_proxy = proxy.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            match Command::parse(&line) {
                Some(Command::Close) => break,
                Some(Command::SetOpacity(v)) => {
                    let _ = stdin_proxy.send_event(UserEvent::SetOpacity(v));
                }
                None => log::warn!("unknown command: {line}"),
            }
        }
        let _ = stdin_proxy.send_event(UserEvent::CloseRequested);
    });

    let sink = {
        let proxy = proxy.clone();
        FrameSink::new(move |event| {
            let _ = proxy.send_event(UserEvent::Capture(event));
        })
    };

    let mut app = PipApp {
        opacity: config.opacity,
        config,
        proxy,
        sink,
        capture: None,
        window: None,
        surface: None,
        frame: None,
        cursor: PhysicalPosition::new(0.0, 0.0),
        hovered: false,
        started: false,
        closing: false,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

impl PipApp {
    fn start_capture(&mut self) {
        let request =
            CaptureRequest { restore_token: self.config.restore_token.clone(), fps: self.config.profile.fps };
        match capture::start(request, self.sink.clone()) {
            Ok(handle) => self.capture = Some(handle),
            Err(reason) => {
                let _ = self.proxy.send_event(UserEvent::Capture(CaptureEvent::Ended { reason }));
            }
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let size = self.config.profile.default_size;
        let mut attrs = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_decorations(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_min_inner_size(LogicalSize::new(120.0, 80.0))
            .with_inner_size(LogicalSize::new(size.width as f64, size.height as f64))
            // Stay hidden until the user has picked a window in the portal dialog.
            .with_visible(false);
        if let Some(g) = self.config.geometry {
            attrs = attrs
                .with_position(PhysicalPosition::new(g.x, g.y))
                .with_inner_size(PhysicalSize::new(g.width, g.height));
        }

        let window = Rc::new(event_loop.create_window(attrs).map_err(|e| e.to_string())?);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
        self.window = Some(window);
        self.surface = Some(surface);
        Ok(())
    }

    fn geometry(&self) -> Option<Geometry> {
        let window = self.window.as_ref()?;
        let pos = window.outer_position().ok()?;
        let size = window.inner_size();
        Some(Geometry { x: pos.x, y: pos.y, width: size.width, height: size.height })
    }

    fn close(&mut self, event_loop: &ActiveEventLoop, reason: &str, by_user: bool) {
        if self.closing {
            return;
        }
        self.closing = true;
        if let Some(mut capture) = self.capture.take() {
            capture.stop();
        }
        super::send(&ChildMessage::Closed {
            reason: reason.to_string(),
            by_user,
            // Only report a geometry the user has actually seen.
            geometry: if self.started { self.geometry() } else { None },
            opacity: self.opacity,
        });
        event_loop.exit();
    }

    fn redraw(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else { return };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else { return };
        match &self.frame {
            Some(frame) => {
                let src = self.config.profile.crop.apply(frame.width, frame.height);
                render::blit(frame, src, &mut buffer, size.width, size.height);
            }
            None => buffer.fill(render::BACKGROUND),
        }
        if self.hovered {
            render::draw_overlay(&mut buffer, size.width, size.height);
        }
        let _ = buffer.present();
    }

    fn resize_direction(&self) -> Option<ResizeDirection> {
        let size = self.window.as_ref()?.inner_size();
        let (x, y) = (self.cursor.x, self.cursor.y);
        let (w, h) = (size.width as f64, size.height as f64);
        let west = x < RESIZE_BORDER;
        let east = x > w - RESIZE_BORDER;
        let north = y < RESIZE_BORDER;
        let south = y > h - RESIZE_BORDER;
        Some(match (north, south, west, east) {
            (true, _, true, _) => ResizeDirection::NorthWest,
            (true, _, _, true) => ResizeDirection::NorthEast,
            (_, true, true, _) => ResizeDirection::SouthWest,
            (_, true, _, true) => ResizeDirection::SouthEast,
            (true, ..) => ResizeDirection::North,
            (_, true, ..) => ResizeDirection::South,
            (_, _, true, _) => ResizeDirection::West,
            (_, _, _, true) => ResizeDirection::East,
            _ => return None,
        })
    }

    fn over_close_button(&self) -> bool {
        let Some(window) = &self.window else { return false };
        render::contains(render::close_button(window.inner_size().width), self.cursor.x, self.cursor.y)
    }

    fn update_cursor_icon(&self) {
        let Some(window) = &self.window else { return };
        let icon = match self.resize_direction() {
            Some(ResizeDirection::North | ResizeDirection::South) => CursorIcon::NsResize,
            Some(ResizeDirection::East | ResizeDirection::West) => CursorIcon::EwResize,
            Some(ResizeDirection::NorthWest | ResizeDirection::SouthEast) => CursorIcon::NwseResize,
            Some(ResizeDirection::NorthEast | ResizeDirection::SouthWest) => CursorIcon::NeswResize,
            None if self.over_close_button() => CursorIcon::Pointer,
            None => CursorIcon::Move,
        };
        window.set_cursor(icon);
    }

    fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(MIN_OPACITY, 1.0);
        if let Some(window) = &self.window {
            platform::set_opacity(window, self.opacity);
        }
    }
}

impl ApplicationHandler<UserEvent> for PipApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.create_window(event_loop) {
            eprintln!("failed to create window: {e}");
            self.close(event_loop, &e, false);
            return;
        }
        self.start_capture();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Capture(CaptureEvent::Started { restore_token }) => {
                self.started = true;
                super::send(&ChildMessage::Started { restore_token });
                if let Some(window) = &self.window {
                    window.set_visible(true);
                    // Some window managers only honour these once the window is mapped.
                    window.set_window_level(WindowLevel::AlwaysOnTop);
                }
                self.set_opacity(self.opacity);
            }
            UserEvent::Capture(CaptureEvent::Frame) => {
                if let Some(frame) = self.sink.take() {
                    self.frame = Some(frame);
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            UserEvent::Capture(CaptureEvent::Ended { reason }) => {
                // The user closing the source window (or cancelling the picker) ends the PiP.
                self.close(event_loop, &reason, true);
            }
            UserEvent::CloseRequested => self.close(event_loop, "closed by the control app", false),
            UserEvent::SetOpacity(v) => self.set_opacity(v),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.close(event_loop, "closed", true),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(_) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                self.update_cursor_icon();
            }
            WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. } => {
                self.hovered = matches!(event, WindowEvent::CursorEntered { .. });
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
                let Some(window) = self.window.clone() else { return };
                match button {
                    MouseButton::Left if self.over_close_button() => self.close(event_loop, "closed", true),
                    MouseButton::Left => {
                        let _ = match self.resize_direction() {
                            Some(direction) => window.drag_resize_window(direction),
                            None => window.drag_window(),
                        };
                    }
                    MouseButton::Middle => self.close(event_loop, "closed", true),
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                self.close(event_loop, "closed", true);
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::cell::RefCell;

    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;

    thread_local! {
        static CONN: RefCell<Option<(RustConnection, u32)>> = const { RefCell::new(None) };
    }

    /// Sets `_NET_WM_WINDOW_OPACITY`, honoured by compositing X11 window managers (incl. Mutter/XWayland).
    pub fn set_opacity(window: &Window, opacity: f32) {
        let id = match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Xlib(h)) => h.window as u32,
            Ok(RawWindowHandle::Xcb(h)) => h.window.get(),
            _ => return, // native Wayland: no client-side opacity protocol
        };
        let result = CONN.with_borrow_mut(|slot| -> Result<(), Box<dyn std::error::Error>> {
            if slot.is_none() {
                let (conn, _) = x11rb::connect(None)?;
                let atom = conn.intern_atom(false, b"_NET_WM_WINDOW_OPACITY")?.reply()?.atom;
                *slot = Some((conn, atom));
            }
            let (conn, atom) = slot.as_ref().expect("connected above");
            let atom = *atom;
            let value = (opacity.clamp(0.0, 1.0) as f64 * u32::MAX as f64) as u32;
            conn.change_property32(PropMode::REPLACE, id, atom, AtomEnum::CARDINAL, &[value])?;
            conn.flush()?;
            Ok(())
        });
        if let Err(e) = result {
            log::warn!("failed to set opacity: {e}");
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod platform {
    pub fn set_opacity(_window: &winit::window::Window, _opacity: f32) {
        // Windows (SetLayeredWindowAttributes) and macOS (NSWindow.alphaValue): Phases 3-4.
    }
}
