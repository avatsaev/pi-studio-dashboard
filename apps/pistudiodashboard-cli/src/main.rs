//! The headless CLI: the command registry and the offline config file.
//!
//! ```sh
//! pistudiodashboard-cli commands --filter feed
//! pistudiodashboard-cli config path
//! pistudiodashboard-cli config show
//! pistudiodashboard-cli config validate [FILE]
//! pistudiodashboard-cli daemon list
//! pistudiodashboard-cli daemon add --name Box --host 127.0.0.1 --port 6790
//! pistudiodashboard-cli daemon remove <ENTRY_ID>
//! ```
//!
//! Everything here is offline: no network connection is ever opened, no keychain is touched and
//! no password is read or printed. `--config FILE` (any `config` or `daemon` subcommand) names a
//! config file instead of the app's; writing needs it to be called `config.json`. Results are
//! JSON lines on standard output; diagnostics go to standard error. Exit status 1 is a failed
//! operation, 2 a usage error.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod offline;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use pistudiodashboard_engine::Session;
use serde_json::json;

#[derive(Parser)]
#[command(name = "pistudiodashboard-cli", version, about = "Pi Studio Dashboard without a window: the command registry and the offline config file")]
struct Cli {
    /// The config file to use instead of the app's (`config.json` in the app's config directory).
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the command registry, one JSON object per line (id, label, menu, internal, enabled, ...).
    Commands {
        /// Only commands whose id or label contains this.
        #[arg(long)]
        filter: Option<String>,
    },
    /// Inspect the config file.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// List, add and remove daemon entries in the config file.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Print the path of the config file.
    Path,
    /// Print the settings and the daemon entries (no passwords exist in the file).
    Show,
    /// Check a config file read-only and print one JSON line per issue; exit 1 if there is any.
    Validate {
        /// The file to check (default: the config file).
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Print the daemon entries.
    List,
    /// Add a daemon entry (validated like `daemon.add` in the app).
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        host: String,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        tls: bool,
        #[arg(long, value_name = "URL")]
        web_ui_url: Option<String>,
    },
    /// Remove a daemon entry by id. A password saved for it in the OS keychain is removed by the
    /// app, not by the CLI.
    Remove { entry_id: String },
}

fn run(cli: Cli) -> Result<(), String> {
    let file = || offline::config_file(cli.config.as_deref());
    match cli.command {
        Command::Commands { filter } => {
            let listed = Session::new().execute("command.list", json!({"filter": filter})).map_err(|e| e.to_string())?;
            for entry in listed.as_array().into_iter().flatten() {
                println!("{entry}");
            }
            Ok(())
        }
        Command::Config { command: ConfigCommand::Path } => {
            println!("{}", file()?.display());
            Ok(())
        }
        Command::Config { command: ConfigCommand::Show } => offline::show(&file()?),
        Command::Config { command: ConfigCommand::Validate { file: given } } => offline::validate(&match given {
            Some(path) => path,
            None => file()?,
        }),
        Command::Daemon { command: DaemonCommand::List } => offline::list(&file()?),
        Command::Daemon { command: DaemonCommand::Add { name, host, port, tls, web_ui_url } } => offline::add(&file()?, &name, &host, port, tls, web_ui_url),
        Command::Daemon { command: DaemonCommand::Remove { entry_id } } => offline::remove(&file()?, &entry_id),
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
