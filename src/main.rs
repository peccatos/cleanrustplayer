// Thin entrypoint that routes CLI commands, local server startup, and explicit DB mode.
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
    dotenvy::dotenv().ok();
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        None => {
            let mut app = App::bootstrap(None)?;
            app.run()
        }
        Some("db") => run_db_mode(&args[1..]),
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
    path.exists() && path.is_file()
}

fn print_usage() {
    println!("ReplayCore CLI");
    println!("Usage:");
    println!("  rust-player serve");
    println!("  rust-player db <status|migrate|sync|serve>");
    println!("  rust-player help");
    println!(
        "  rust-player <contract|snapshot|status|list|queue|find|queuefind|search|resolve|providers|provider> ..."
    );
    println!("  rust-player <play|playname|open|next|prev|pause|resume|stop|volume|seek|pos|repeat|shuffle|reload> ...");
    println!("  rust-player <audio-file-path>");
    println!();
    println!("Top-level single-shot commands exit after running.");
    println!("Playback commands start or reuse the interactive shell.");
}

fn run_db_mode(args: &[String]) -> Result<()> {
    match args.first().map(|s| s.as_str()) {
        None | Some("help") | Some("-h") | Some("--help") => {
            print_db_usage();
            Ok(())
        }
        Some("status") => {
            let context = AppContext::bootstrap_database()?;
            print_db_status(&context);
            Ok(())
        }
        Some("migrate") => {
            let context = AppContext::bootstrap_database()?;
            println!("database ready: {}", context.user_id);
            Ok(())
        }
        Some("sync") => {
            let mut context = AppContext::bootstrap_database()?;
            let tracks = context.reload_local_library()?;
            println!("synced: {} track(s)", tracks);
            Ok(())
        }
        Some("serve") => {
            let context = AppContext::bootstrap_database()?;
            let addr = env::var("REPLAYCORE_ADDR")
                .ok()
                .and_then(|value| value.parse::<SocketAddr>().ok())
                .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 3000)));

            run_server(context, addr)
        }
        Some(other) => {
            eprintln!("unknown db command: {}", other);
            print_db_usage();
            Ok(())
        }
    }
}

fn print_db_usage() {
    println!("ReplayCore DB mode");
    println!("Usage:");
    println!("  rust-player db status");
    println!("  rust-player db migrate");
    println!("  rust-player db sync");
    println!("  rust-player db serve");
}

fn print_db_status(context: &AppContext) {
    println!("user_id: {}", context.user_id);
    println!("tracks: {}", context.tracks.len());
    println!("sources: {}", context.catalog.sources.len());
    println!("saved: {}", context.saved_track_ids.len());
    println!("hidden: {}", context.hidden_track_ids.len());
    println!("roots: {}", context.local_music_roots.len());
}
