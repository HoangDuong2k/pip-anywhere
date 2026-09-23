// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pip_anywhere_lib::pip;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // The same binary is re-launched as `pip-anywhere pip --config <json>` for each PiP window.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(pip::SUBCOMMAND) {
        std::process::exit(pip::run_child(&args[2..]));
    }
    pip_anywhere_lib::run()
}
