//! System tray: pop out a window with a profile, close or reopen PiPs, quit.

use tauri::menu::{Menu, MenuBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager as _, Wry};

use crate::manager::Manager;

const TRAY_ID: &str = "main";

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("PiP Anywhere")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| handle(app, event.id().as_ref()));
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => log::warn!("failed to rebuild tray menu: {e}"),
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = app.state::<Manager>().state();

    let mut pop = SubmenuBuilder::new(app, "Pop out window");
    for p in &state.profiles {
        pop = pop.text(format!("pop:{}", p.id), &p.name);
    }
    let mut menu = MenuBuilder::new(app).item(&pop.build()?);

    let closed: Vec<_> = state.saved.iter().filter(|s| !s.running).collect();
    if !closed.is_empty() {
        let mut reopen = SubmenuBuilder::new(app, "Reopen");
        for s in closed {
            reopen = reopen.text(format!("reopen:{}", s.id), &s.label);
        }
        menu = menu.item(&reopen.build()?);
    }

    if !state.running.is_empty() {
        menu = menu.separator();
        for s in &state.running {
            menu = menu.text(format!("close:{}", s.id), format!("Close “{}”", s.label));
        }
    }

    menu.separator().text("show", "Open control panel").text("quit", "Quit").build()
}

fn handle(app: &AppHandle, id: &str) {
    let manager = app.state::<Manager>();
    let result = if let Some(profile) = id.strip_prefix("pop:") {
        manager.pop_out(profile).map(drop)
    } else if let Some(source) = id.strip_prefix("reopen:") {
        manager.reopen(source).map(drop)
    } else if let Some(source) = id.strip_prefix("close:") {
        manager.close(source);
        Ok(())
    } else if id == "show" {
        crate::show_main_window(app);
        Ok(())
    } else if id == "quit" {
        crate::quit(app);
        Ok(())
    } else {
        Ok(())
    };
    if let Err(e) = result {
        let _ = app.emit("pip-error", e);
    }
}
