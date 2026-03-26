mod api;
mod app;
mod command;
mod context;
mod contract;
mod music;
mod player;
mod provider;
mod provider_accounts;
mod queue;
mod repository;
mod search;
mod service;
mod snapshot;
mod token_vault;

use std::env;
use std::net::SocketAddr;

use crate::api::run_server;
use crate::app::App;
use crate::command::Command;
use crate::context::AppContext;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        None => {
            let mut app = App::bootstrap(None)?;
            app.run()
        }
        Some("serve") | Some("--serve") => {
            let context = AppContext::bootstrap()?;
            let addr = env::var("REPLAYCORE_ADDR")
                .ok()
                .and_then(|value| value.parse::<SocketAddr>().ok())
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3000)));

            run_server(context, addr)
        }
        Some(command) if is_read_only_command(command) => {
            let mut app = App::bootstrap(None)?;
            app.execute_parsed_command(Command::parse_parts(args))?;
            Ok(())
        }
        Some(command) if is_playback_command(command) => {
            let mut app = App::bootstrap(None)?;
            app.execute_parsed_command(Command::parse_parts(args))?;
            app.run()
        }
        Some(source) if looks_like_audio_source(source) => {
            let mut app = App::bootstrap(Some(source.to_string()))?;
            app.run()
        }
        Some(command) if command == "help" || command == "-h" || command == "--help" => {
            print_usage();
            Ok(())
        }
        Some(other) => {
            eprintln!("unknown command or source: {}", other);
            print_usage();
            Ok(())
        }
    }
}

fn is_read_only_command(command: &str) -> bool {
    matches!(
        command,
        "contract"
            | "snapshot"
            | "status"
            | "list"
            | "queue"
            | "find"
            | "queuefind"
            | "search"
            | "resolve"
            | "providers"
            | "provider"
    )
}

fn is_playback_command(command: &str) -> bool {
    matches!(
        command,
        "open"
            | "play"
            | "playname"
            | "playurl"
            | "next"
            | "prev"
            | "pause"
            | "resume"
            | "stop"
            | "volume"
            | "seek"
            | "pos"
            | "repeat"
            | "shuffle"
            | "reload"
    )
}

fn looks_like_audio_source(source: &str) -> bool {
    let source = source.trim();
    if source.is_empty() {
        return false;
    }

    let path = std::path::Path::new(source);
    path.exists()
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| {
                matches!(
                    ext.to_ascii_lowercase().as_str(),
                    "mp3" | "flac" | "wav" | "m4a" | "ogg"
                )
            })
            .unwrap_or(false)
}

fn print_usage() {
    println!("ReplayCore CLI");
    println!("Usage:");
    println!("  rust-player serve");
    println!("  rust-player help");
    println!(
        "  rust-player <contract|snapshot|status|list|queue|find|queuefind|search|resolve|providers|provider> ..."
    );
    println!("  rust-player <play|playname|playurl|open|next|prev|pause|resume|stop|volume|seek|pos|repeat|shuffle|reload> ...");
    println!("  rust-player <audio-file-path>");
    println!();
    println!("Top-level single-shot commands exit after running.");
    println!("Playback commands start or reuse the interactive shell.");
}
