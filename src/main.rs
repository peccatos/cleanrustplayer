use std::env;

use anyhow::Result;

use rust_player::app::App;
use rust_player::command::Command;
use rust_player::config::AppConfig;
use rust_player::context::AppContext;
use rust_player::web;

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let config = AppConfig::from_env()?;
    let args: Vec<String> = env::args().skip(1).collect();

    match args.first().map(|s| s.as_str()) {
        None => {
            let context = AppContext::bootstrap_local_music(&config)?;
            if context.tracks.is_empty() {
                println!("вставьте путь");
            }
            let mut app = App::from_context(context, None)?;
            app.run()
        }
        Some("serve") => run_serve_mode(config),
        Some("db") => run_db_mode(&args[1..]),
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
    println!("  rust-player");
    println!("  rust-player serve");
    println!("  rust-player db <status|migrate|sync>");
    println!("  rust-player help");
    println!("  rust-player <audio-file-path>");
    println!();
    println!("Top-level single-shot commands exit after running.");
    println!("No-arg start uses the local library in ./library by default.");
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
        Some(other) => {
            eprintln!("unknown db command: {}", other);
            print_db_usage();
            Ok(())
        }
    }
}

fn run_serve_mode(config: AppConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    runtime.block_on(web::serve(config))
}

fn print_db_usage() {
    println!("ReplayCore DB mode");
    println!("Usage:");
    println!("  rust-player db status");
    println!("  rust-player db migrate");
    println!("  rust-player db sync");
}

fn print_db_status(context: &AppContext) {
    println!("user_id: {}", context.user_id);
    println!("tracks: {}", context.tracks.len());
    println!("sources: {}", context.catalog.sources.len());
    println!("saved: {}", context.saved_track_ids.len());
    println!("hidden: {}", context.hidden_track_ids.len());
    println!("roots: {}", context.local_music_roots.len());
}


//у нас слишком много зависимостей, и мы не хотим их все грузить в db режиме, так что там будет свой контекст без музыки и провайдеров. Поэтому эти команды будут работать только в полном режиме. Но это не страшно, потому что они нужны только для отладки и администрирования, а для этого полный режим вполне подходит.
//смотри, если уже есть зависимая библиотека, мы можем разветвление и направить #cloud-lib.rs и прописать код для синхронизации с облаком.
