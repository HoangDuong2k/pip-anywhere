mod capture;
mod manager;
pub mod pip;
pub mod profiles;
mod settings;
mod tray;

use std::time::Duration;

use tauri::{AppHandle, Manager as _, State, WindowEvent};

use manager::{Manager, StateView};

#[tauri::command]
fn get_state(manager: State<'_, Manager>) -> StateView {
    manager.state()
}

#[tauri::command]
fn pop_out(manager: State<'_, Manager>, profile_id: String) -> Result<String, String> {
    manager.pop_out(&profile_id)
}

#[tauri::command]
fn reopen(manager: State<'_, Manager>, id: String) -> Result<String, String> {
    manager.reopen(&id)
}

#[tauri::command]
fn close_pip(manager: State<'_, Manager>, id: String) {
    manager.close(&id);
}

#[tauri::command]
fn set_opacity(manager: State<'_, Manager>, id: String, opacity: f32) -> Result<(), String> {
    manager.set_opacity(&id, opacity)
}

#[tauri::command]
fn forget(manager: State<'_, Manager>, id: String) {
    manager.forget(&id);
}

#[tauri::command]
fn rename_source(manager: State<'_, Manager>, id: String, label: String) -> Result<(), String> {
    manager.rename(&id, &label)
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    quit(&app);
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Closes the PiPs (remembering which were open) and exits.
pub(crate) fn quit(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        app.state::<Manager>().close_all_and_wait(Duration::from_secs(2));
        app.exit(0);
    });
}

/// Handles `--pop [profile]` (e.g. bound to a desktop keyboard shortcut) and `--hidden`.
fn handle_args(app: &AppHandle, args: &[String], second_instance: bool) {
    if let Some(i) = args.iter().position(|a| a == "--pop") {
        let profile = args.get(i + 1).filter(|a| !a.starts_with("--")).map_or("default", String::as_str);
        if let Err(e) = app.state::<Manager>().pop_out(profile) {
            log::error!("pop out failed: {e}");
        }
    } else if second_instance || !args.iter().any(|a| a == "--hidden") {
        show_main_window(app);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: forwards `--pop` from a second launch to the running app.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            handle_args(app, &args, true);
        }))
        .invoke_handler(tauri::generate_handler![
            get_state,
            pop_out,
            reopen,
            close_pip,
            set_opacity,
            forget,
            rename_source,
            quit_app
        ])
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("state.json");
            app.manage(Manager::new(app.handle().clone(), path));
            tray::create(app.handle())?;
            let args: Vec<String> = std::env::args().collect();
            handle_args(app.handle(), &args, false);
            app.state::<Manager>().restore_on_start();
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the control panel keeps the app (and its PiPs) running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
