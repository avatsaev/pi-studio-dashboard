//! Pi Studio Dashboard desktop app: a Tauri window whose webview draws the shell's view.
//!
//! Usage: `pistudiodashboard [options]`; `pistudiodashboard --help` lists the options.
//!
//! `--headless` (with `--control`) runs everything but the window, for automation and smoke tests.
//!
//! `--control <port>` (or `PISTUDIODASHBOARD_CONTROL_PORT`) starts a loopback JSON-lines control server:
//! the first line authenticates, then each request line gets one reply line. The methods are
//! in `pistudiodashboard_shell::control`.

// Release builds on Windows are GUI-subsystem apps: no console window from the Start menu.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod bridge;
mod commands;
mod control_server;
mod crash_guard;
mod detect_local;
mod feed_pump;
mod io_worker;
mod keychain;
mod logging;
mod owner;
#[cfg(test)]
mod pipeline_tests;
mod runtime;
mod services;
mod startup;
mod view_emitter;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::channel;

use clap::Parser;
use tauri::Emitter;

#[derive(Parser)]
#[command(name = "pistudiodashboard", version, about = "Pi Studio Dashboard")]
struct Args {
    /// Serve the JSON control protocol on 127.0.0.1:<PORT> (0 = any free port).
    #[arg(long, env = "PISTUDIODASHBOARD_CONTROL_PORT", value_name = "PORT")]
    control: Option<u16>,
    /// File holding the control token (64 hex characters); created with a fresh token when missing.
    #[arg(long, env = "PISTUDIODASHBOARD_CONTROL_TOKEN_FILE", value_name = "FILE")]
    control_token_file: Option<PathBuf>,
    /// Use this config directory instead of the platform default.
    #[arg(long, env = "PISTUDIODASHBOARD_CONFIG_DIR", value_name = "DIR")]
    config_dir: Option<PathBuf>,
    /// Run without a window: the engine, the daemon connections and the control server only.
    /// Needs `--control`.
    #[arg(long, requires = "control")]
    headless: bool,
}

fn main() {
    let args = Args::parse();
    logging::init();
    crash_guard::install_hook();
    // Bind before the window opens: a port in use is a startup error, not a silent no-op.
    let control = match args.control.map(|port| control_server::bind(port, args.control_token_file.as_deref())) {
        Some(Ok(bound)) => Some(bound),
        Some(Err(e)) => {
            eprintln!("pistudiodashboard: {e}");
            std::process::exit(2);
        }
        None => None,
    };
    // Every request, from the webview, the control socket, the feed pump and the I/O worker,
    // crosses this channel to the one thread that owns the session.
    let (requests, inbox) = channel();
    if let Some((listener, token)) = control {
        control_server::start(listener, token, requests.clone());
    }
    let config_dir = args.config_dir.or_else(|| pistudiodashboard_config::config_dir(&|k| std::env::var(k).ok()));
    if config_dir.is_none() {
        log::warn!("no config directory could be determined: settings will not be saved");
    }
    let worker = Arc::new(io_worker::IoWorker::start(config_dir.clone(), requests.clone(), Arc::new(keychain::OsStore)));
    let client = runtime::start_client(requests.clone());
    let wiring = owner::Wiring { client, worker: Arc::clone(&worker), config_dir };

    if args.headless {
        let app = owner::build(&wiring);
        owner::run(&inbox, app, |_| {});
        worker.shutdown();
        return;
    }
    let result = tauri::Builder::default()
        .manage(bridge::Bridge::new(requests))
        .invoke_handler(tauri::generate_handler![commands::control, commands::invoke_command])
        .setup(move |app| {
            let handle = app.handle().clone();
            std::thread::Builder::new().name("pistudiodashboard-owner".into()).spawn(move || {
                let shell = owner::build(&wiring);
                owner::run(&inbox, shell, |view| {
                    if let Err(e) = handle.emit(bridge::VIEW_EVENT, view) {
                        log::warn!("cannot send the view to the window: {e}");
                    }
                });
            })?;
            Ok(())
        })
        .run(tauri::generate_context!());
    worker.shutdown();
    if let Err(e) = result {
        eprintln!("pistudiodashboard: {e}");
        std::process::exit(1);
    }
}
