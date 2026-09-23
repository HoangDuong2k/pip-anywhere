//! Linux capture: xdg-desktop-portal ScreenCast (window picker) + PipeWire stream.
//! Works on Wayland (GNOME, KDE, wlroots) and on X11 desktops that ship the portal.

use std::cell::Cell;
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::desktop::PersistMode;
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};

use super::{CaptureEvent, CaptureHandle, CaptureRequest, FrameSink, PixelFormat};

pub fn start(req: CaptureRequest, sink: FrameSink) -> Result<CaptureHandle, String> {
    let (quit_tx, quit_rx) = pw::channel::channel::<()>();

    std::thread::Builder::new()
        .name("capture".into())
        .spawn(move || {
            let reason = match run(req, &sink, quit_rx) {
                Ok(()) => "stopped".to_string(),
                Err(e) => e,
            };
            sink.notify(CaptureEvent::Ended { reason });
        })
        .map_err(|e| e.to_string())?;

    Ok(CaptureHandle::new(move || {
        let _ = quit_tx.send(());
    }))
}

fn run(req: CaptureRequest, sink: &FrameSink, quit_rx: pw::channel::Receiver<()>) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;

    rt.block_on(async {
        let proxy = Screencast::new().await.map_err(portal_err)?;
        let session = proxy.create_session().await.map_err(portal_err)?;
        proxy
            .select_sources(
                &session,
                CursorMode::Embedded,
                SourceType::Window.into(),
                false,
                req.restore_token.as_deref(),
                PersistMode::ExplicitlyRevoked,
            )
            .await
            .map_err(portal_err)?;

        let streams =
            proxy.start(&session, None).await.map_err(portal_err)?.response().map_err(portal_err)?;
        let node_id = streams.streams().first().ok_or("the portal returned no stream")?.pipe_wire_node_id();
        let fd = proxy.open_pipe_wire_remote(&session).await.map_err(portal_err)?;

        sink.notify(CaptureEvent::Started { restore_token: streams.restore_token().map(str::to_owned) });

        // Blocks until the stream ends or stop is requested. `session` stays alive meanwhile.
        let result = stream_frames(node_id, fd, quit_rx, sink, req.fps);
        let _ = session.close().await;
        result
    })
}

fn portal_err(e: ashpd::Error) -> String {
    match e {
        ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled) => "cancelled".into(),
        e => format!("portal error: {e}"),
    }
}

struct StreamData {
    format: VideoInfoRaw,
    next_frame_at: Instant,
    warned_format: bool,
}

fn stream_frames(
    node_id: u32,
    fd: OwnedFd,
    quit_rx: pw::channel::Receiver<()>,
    sink: &FrameSink,
    fps: u32,
) -> Result<(), String> {
    pw::init();
    let err = |e: pw::Error| format!("pipewire error: {e}");

    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(err)?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(err)?;
    let core = context.connect_fd_rc(fd, None).map_err(err)?;

    let _quit = quit_rx.attach(mainloop.loop_(), {
        let mainloop = mainloop.clone();
        move |_| mainloop.quit()
    });

    let ended: Rc<Cell<Option<&'static str>>> = Rc::new(Cell::new(None));
    let frame_interval = Duration::from_secs_f64(1.0 / fps.max(1) as f64);
    let sink = sink.clone();

    let stream = pw::stream::StreamBox::new(
        &core,
        "pip-anywhere",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(err)?;

    let _listener = stream
        .add_local_listener_with_user_data(StreamData {
            format: Default::default(),
            next_frame_at: Instant::now(),
            warned_format: false,
        })
        .state_changed({
            let mainloop = mainloop.clone();
            let ended = ended.clone();
            move |_, _, old, new| {
                log::debug!("pipewire stream: {old:?} -> {new:?}");
                use pw::stream::StreamState::*;
                let gone = matches!((&old, &new), (_, Error(_)) | (Streaming | Paused, Unconnected));
                if gone {
                    ended.set(Some("the source window was closed"));
                    mainloop.quit();
                }
            }
        })
        .param_changed(|_, data, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, media_subtype)) = spa::param::format_utils::parse_format(param) else {
                return;
            };
            if media_type != spa::param::format::MediaType::Video
                || media_subtype != spa::param::format::MediaSubtype::Raw
            {
                return;
            }
            if data.format.parse(param).is_ok() {
                log::info!(
                    "stream format {:?} {}x{}",
                    data.format.format(),
                    data.format.size().width,
                    data.format.size().height
                );
            }
        })
        .process(move |stream, data| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let now = Instant::now();
            if now < data.next_frame_at {
                return; // throttle to the profile frame rate
            }

            let format = match data.format.format() {
                VideoFormat::BGRx | VideoFormat::BGRA => PixelFormat::Bgrx,
                VideoFormat::RGBx | VideoFormat::RGBA => PixelFormat::Rgbx,
                other => {
                    if !data.warned_format {
                        log::warn!("unsupported pixel format {other:?}");
                        data.warned_format = true;
                    }
                    return;
                }
            };
            let (width, height) = (data.format.size().width, data.format.size().height);

            let datas = buffer.datas_mut();
            let Some(plane) = datas.first_mut() else { return };
            let chunk = plane.chunk();
            let offset = chunk.offset() as usize;
            let stride = match chunk.stride() {
                s if s > 0 => s as usize,
                _ => width as usize * 4,
            };
            let needed = stride * height as usize;
            let Some(bytes) = plane.data() else { return }; // e.g. DMA-BUF, not mapped
            let Some(bytes) = bytes.get(offset..offset + needed) else { return };

            data.next_frame_at = now + frame_interval;
            sink.publish(width, height, stride, format, bytes);
        })
        .register()
        .map_err(err)?;

    let format_pod = video_format_pod(fps)?;
    let mut params = [spa::pod::Pod::from_bytes(&format_pod).ok_or("invalid format pod")?];
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(err)?;

    mainloop.run();

    match ended.get() {
        Some(reason) => Err(reason.to_string()),
        None => Ok(()),
    }
}

/// Offers packed 32-bit RGB formats only; without DRM modifiers the compositor
/// falls back to shared-memory buffers we can read directly.
fn video_format_pod(fps: u32) -> Result<Vec<u8>, String> {
    let obj = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle { width: 1280, height: 720 },
            spa::utils::Rectangle { width: 1, height: 1 },
            spa::utils::Rectangle { width: 8192, height: 8192 }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: fps.max(1), denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 240, denom: 1 }
        ),
    );
    let (cursor, _) = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| format!("failed to build format pod: {e:?}"))?;
    Ok(cursor.into_inner())
}
