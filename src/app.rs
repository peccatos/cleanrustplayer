use std::env;
use std::io::{self, Write};

use anyhow::Result;

use crate::command::{Command, RepeatModeArg};
use crate::music::library::{
    format_duration, load_default_library, print_tracks, resolve_initial_track, Track,
};
use crate::player::AudioPlayer;
use crate::provider::bandcamp::BandcampProvider;
use crate::provider::registry::ProviderRegistry;
use crate::provider::{ProviderHttpConfig, ProviderKind, SearchQuery};
use crate::queue::PlaybackQueue;
use crate::search::SearchService;
use crate::snapshot::{AppSnapshot, NowPlayingView, QueueEntryView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepeatMode {
    Off,
    One,
    All,
}

pub struct App {
    player: AudioPlayer,
    tracks: Vec<Track>,
    current_index: Option<usize>,
    repeat_mode: RepeatMode,
    queue: PlaybackQueue,
    search_service: SearchService,
}

impl App {
    pub fn bootstrap() -> Result<Self> {
        let mut args = env::args().skip(1);

        let tracks = load_default_library()?;
        let initial_track = resolve_initial_track(args.next(), &tracks)?;
        let current_index = tracks.iter().position(|t| t.path == initial_track.path);

        let mut player = AudioPlayer::new()?;
        player.load_and_play(&initial_track.path)?;

        let queue = PlaybackQueue::new(tracks.len(), current_index, false);

        let mut registry = ProviderRegistry::new();
        registry.register(BandcampProvider::new(ProviderHttpConfig::default())?);
        let search_service = SearchService::new(registry);

        println!("Loaded: {}", initial_track.display_label());
        println!("Path: {}", initial_track.path.display());
        println!("Duration: {}", initial_track.duration_label());

        Ok(Self {
            player,
            tracks,
            current_index,
            repeat_mode: RepeatMode::Off,
            queue,
            search_service,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        self.print_help();
        let stdin = io::stdin();

        loop {
            self.autonext_if_needed()?;
            print!("> ");
            io::stdout().flush()?;

            let mut line = String::new();
            stdin.read_line(&mut line)?;
            let input = line.trim();
            if input.is_empty() {
                continue;
            }

            if !self.handle_command(input)? {
                break;
            }
        }

        Ok(())
    }

    fn handle_command(&mut self, input: &str) -> Result<bool> {
        match Command::parse(input) {
            Command::List => print_tracks(&self.tracks),
            Command::Queue => self.print_queue(),
            Command::Find(query) => self.print_find_results(&query),
            Command::QueueFind(query) => self.print_queue_find_results(&query),
            Command::Search(query) => self.print_provider_search_results(&query),
            Command::Resolve(url) => self.print_resolved_media(&url),
            Command::PlayUrl(url) => self.play_url(&url)?,

            Command::Play(index) => {
                let Some(track) = self.tracks.get(index).cloned() else {
                    println!("track index out of range");
                    return Ok(true);
                };
                self.play_track(index, &track)?;
            }

            Command::PlayName(query) => match self.find_first_track_index(&query) {
                Some(index) => {
                    let track = self.tracks[index].clone();
                    self.play_track(index, &track)?;
                }
                None => println!("no track matched query: {}", query),
            },

            Command::Next => self.next_track()?,
            Command::Prev => self.prev_track()?,
            Command::Pause => {
                self.player.pause();
                println!("paused");
            }
            Command::Resume => {
                self.player.resume();
                println!("resumed");
            }
            Command::Stop => {
                self.player.stop();
                println!("stopped");
            }
            Command::Volume(volume) => {
                if !(0.0..=1.0).contains(&volume) {
                    println!("volume must be 0..1");
                    return Ok(true);
                }
                self.player.set_volume(volume);
                println!("volume: {}", self.player.volume());
            }
            Command::Seek(seconds) => match self.player.seek_to(seconds) {
                Ok(()) => println!("seeked to {}s", seconds),
                Err(err) => println!("seek error: {}", err),
            },
            Command::Pos => println!("position: {:.2}s", self.player.position().as_secs_f32()),
            Command::Repeat(mode) => {
                self.repeat_mode = match mode {
                    RepeatModeArg::Off => RepeatMode::Off,
                    RepeatModeArg::One => RepeatMode::One,
                    RepeatModeArg::All => RepeatMode::All,
                };
                println!("repeat: {}", self.repeat_mode_label());
            }
            Command::Shuffle(enabled) => {
                self.queue
                    .set_shuffle(enabled, self.tracks.len(), self.current_index);
                println!(
                    "shuffle: {}",
                    if self.queue.is_shuffle_enabled() {
                        "on"
                    } else {
                        "off"
                    }
                );
            }
            Command::Status => self.print_status(),
            Command::Snapshot => self.snapshot().print_json(),
            Command::Reload => self.reload_library()?,
            Command::Help => self.print_help(),
            Command::Exit => {
                self.player.stop();
                return Ok(false);
            }
            Command::Unknown(cmd) => println!("unknown or invalid command: {}", cmd),
        }

        Ok(true)
    }

    fn print_provider_search_results(&self, query: &str) {
        let report = self.search_service.search(SearchQuery::new(query, 10));

        if report.query.is_empty() {
            println!("empty query");
            return;
        }

        println!("Search results:");
        if report.items.is_empty() {
            println!("  no items");
        } else {
            for (i, item) in report.items.iter().enumerate() {
                println!(
                    "  [{}] [{}:{}] {}",
                    i,
                    item.provider,
                    item.kind,
                    item.display_title()
                );
                println!("      url: {}", item.url);
            }
        }

        if !report.errors.is_empty() {
            println!("Provider errors:");
            for err in report.errors {
                println!("  [{}] {}", err.provider, err.message);
            }
        }
    }

    fn print_resolved_media(&self, url: &str) {
        let normalized = url.trim();
        if normalized.is_empty() {
            println!("empty url");
            return;
        }

        let Some(provider) = self.search_service.registry().find(ProviderKind::Bandcamp) else {
            println!("bandcamp provider is not registered");
            return;
        };

        match provider.resolve_page(normalized) {
            Ok(media) => {
                println!("Resolved:");
                println!("  provider: {}", media.provider);
                println!("  kind: {}", media.kind);
                println!("  title: {}", media.title);
                println!(
                    "  artist: {}",
                    media.artist.unwrap_or_else(|| "<none>".to_string())
                );
                println!("  page_url: {}", media.page_url);
                println!("  playable: {}", media.playable);
                println!(
                    "  preview_url: {}",
                    media.preview_url.unwrap_or_else(|| "<none>".to_string())
                );
            }
            Err(err) => {
                println!("resolve error: {}", err);
            }
        }
    }

    fn play_url(&mut self, url: &str) -> Result<()> {
        let normalized = url.trim();
        if normalized.is_empty() {
            println!("empty url");
            return Ok(());
        }

        self.player.load_url_and_play(normalized)?;
        self.current_index = None;

        println!("Loaded remote stream");
        println!("URL: {}", normalized);

        Ok(())
    }

    fn reload_library(&mut self) -> Result<()> {
        self.tracks = load_default_library()?;
        self.current_index = self
            .player
            .current_path()
            .and_then(|p| self.tracks.iter().position(|t| t.path.as_path() == p));
        self.queue.rebuild(self.tracks.len(), self.current_index);
        println!("reloaded: {} tracks", self.tracks.len());
        Ok(())
    }

    fn print_status(&self) {
        let s = self.snapshot();
        println!("file: {}", s.now_playing.file_path);
        println!("track: {}", s.now_playing.label);
        println!("artist: {}", s.now_playing.artist);
        println!("album: {}", s.now_playing.album);
        println!("duration: {}", s.now_playing.duration_label);
        println!("current_index: {:?}", s.now_playing.library_index);
        println!("queue_position: {:?}", s.queue_position);
        println!("queue_len: {}", s.queue_len);
        println!("paused: {}", s.now_playing.paused);
        println!("empty: {}", s.now_playing.empty);
        println!("position_sec: {:.2}", s.now_playing.position_sec);
        println!("volume: {}", s.now_playing.volume);
        println!("repeat: {}", s.repeat_mode);
        println!("shuffle: {}", if s.shuffle_enabled { "on" } else { "off" });
        println!("tracks_scanned: {}", s.tracks_scanned);
    }

    fn print_queue(&self) {
        let s = self.snapshot();
        if s.queue.is_empty() {
            println!("Queue is empty");
            return;
        }
        println!("Queue:");
        for e in s.queue {
            let marker = if e.is_current { ">" } else { " " };
            println!(
                "{} [{:02}] track={} {} ({})",
                marker, e.queue_position, e.library_index, e.label, e.duration_label
            );
        }
    }

    fn print_find_results(&self, query: &str) {
        let q = normalize_query(query);
        if q.is_empty() {
            println!("empty query");
            return;
        }

        let mut found = 0usize;
        println!("Find results:");
        for (index, track) in self.tracks.iter().enumerate() {
            if self.track_matches_query(track, &q) {
                println!(
                    "  [{}] {} ({})",
                    index,
                    track.display_label(),
                    track.duration_label()
                );
                found += 1;
            }
        }

        if found == 0 {
            println!("  no matches");
        } else {
            println!("  total: {}", found);
        }
    }

    fn print_queue_find_results(&self, query: &str) {
        let q = normalize_query(query);
        if q.is_empty() {
            println!("empty query");
            return;
        }

        let mut found = 0usize;
        println!("Queue find results:");
        for (pos, &track_index) in self.queue.entries().iter().enumerate() {
            let track = &self.tracks[track_index];
            if self.track_matches_query(track, &q) {
                let marker = if Some(track_index) == self.current_index {
                    ">"
                } else {
                    " "
                };
                println!(
                    "{} [{:02}] track={} {} ({})",
                    marker,
                    pos,
                    track_index,
                    track.display_label(),
                    track.duration_label()
                );
                found += 1;
            }
        }

        if found == 0 {
            println!("  no matches");
        } else {
            println!("  total: {}", found);
        }
    }

    fn find_first_track_index(&self, query: &str) -> Option<usize> {
        let q = normalize_query(query);
        if q.is_empty() {
            return None;
        }

        self.tracks
            .iter()
            .enumerate()
            .find(|(_, t)| self.track_matches_query(t, &q))
            .map(|(i, _)| i)
    }

    fn track_matches_query(&self, track: &Track, query: &str) -> bool {
        track.file_name.to_lowercase().contains(query)
            || track.display_label().to_lowercase().contains(query)
            || track
                .title
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(query)
            || track
                .artist
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(query)
            || track
                .album
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(query)
    }

    fn snapshot(&self) -> AppSnapshot {
        let current = self.current_index.and_then(|i| self.tracks.get(i));

        let file_or_url = self
            .player
            .current_path()
            .map(|p| p.display().to_string())
            .or_else(|| self.player.current_url().map(|u| u.to_string()))
            .unwrap_or_else(|| "<none>".to_string());

        let now_playing = NowPlayingView {
            library_index: self.current_index,
            label: current
                .map(|t| t.display_label())
                .unwrap_or_else(|| file_or_url.clone()),
            artist: current
                .and_then(|t| t.artist.clone())
                .unwrap_or_else(|| "<none>".to_string()),
            album: current
                .and_then(|t| t.album.clone())
                .unwrap_or_else(|| "<none>".to_string()),
            duration_label: current
                .and_then(|t| t.duration)
                .map(format_duration)
                .unwrap_or_else(|| "--:--".to_string()),
            file_path: file_or_url,
            position_sec: self.player.position().as_secs_f32(),
            paused: self.player.is_paused(),
            empty: self.player.is_empty(),
            volume: self.player.volume(),
        };

        let queue = self
            .queue
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(queue_position, &library_index)| {
                self.tracks.get(library_index).map(|track| QueueEntryView {
                    queue_position,
                    library_index,
                    is_current: Some(library_index) == self.current_index,
                    label: track.display_label(),
                    duration_label: track.duration_label(),
                })
            })
            .collect();

        AppSnapshot {
            repeat_mode: self.repeat_mode_label().to_string(),
            shuffle_enabled: self.queue.is_shuffle_enabled(),
            tracks_scanned: self.tracks.len(),
            queue_len: self.queue.len(),
            queue_position: self.queue.position(),
            now_playing,
            queue,
        }
    }

    fn play_track(&mut self, index: usize, track: &Track) -> Result<()> {
        self.player.load_and_play(&track.path)?;
        self.current_index = Some(index);
        self.queue.set_current_track(index);

        println!("Loaded: {}", track.display_label());
        println!("Path: {}", track.path.display());
        println!("Duration: {}", track.duration_label());

        Ok(())
    }

    fn next_track(&mut self) -> Result<()> {
        let Some(next) = self.compute_next_index() else {
            println!("no next track");
            return Ok(());
        };

        let track = self.tracks[next].clone();
        self.play_track(next, &track)
    }

    fn prev_track(&mut self) -> Result<()> {
        let Some(prev) = self.compute_prev_index() else {
            println!("no previous track");
            return Ok(());
        };

        let track = self.tracks[prev].clone();
        self.play_track(prev, &track)
    }

    fn compute_next_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }

        match self.repeat_mode {
            RepeatMode::One => self.current_index,
            RepeatMode::Off => self.queue.next_track_index(),
            RepeatMode::All => self
                .queue
                .next_track_index()
                .or_else(|| self.queue.first_track_index()),
        }
    }

    fn compute_prev_index(&self) -> Option<usize> {
        if self.queue.is_empty() {
            return None;
        }

        match self.repeat_mode {
            RepeatMode::One => self.current_index,
            RepeatMode::Off => self.queue.prev_track_index(),
            RepeatMode::All => self
                .queue
                .prev_track_index()
                .or_else(|| self.queue.last_track_index()),
        }
    }

    fn autonext_if_needed(&mut self) -> Result<()> {
        if self.current_index.is_none()
            || self.player.is_paused()
            || self.player.current_path().is_none()
            || !self.player.is_empty()
        {
            return Ok(());
        }

        if let Some(next) = self.compute_next_index() {
            let track = self.tracks[next].clone();
            self.play_track(next, &track)?;
        } else {
            self.player.stop();
        }

        Ok(())
    }

    fn repeat_mode_label(&self) -> &'static str {
        match self.repeat_mode {
            RepeatMode::Off => "off",
            RepeatMode::One => "one",
            RepeatMode::All => "all",
        }
    }

    fn print_help(&self) {
        println!("Commands:");
        println!("  list                  - show scanned tracks");
        println!("  queue                 - show playback queue");
        println!("  find <query>          - search local library");
        println!("  queuefind <query>     - search current queue");
        println!("  search <query>        - search provider layer");
        println!("  resolve <url>         - resolve provider page into preview stream");
        println!("  playurl <url>         - download remote stream and play it");
        println!("  play <index>          - play track by library index");
        println!("  playname <query>      - play first library match");
        println!("  next                  - play next track in queue");
        println!("  prev                  - play previous track in queue");
        println!("  pause                 - pause playback");
        println!("  resume                - resume playback");
        println!("  stop                  - stop playback");
        println!("  volume <0..1>         - set volume");
        println!("  seek <sec>            - seek in current source");
        println!("  pos                   - show current position");
        println!("  repeat off|one|all    - set repeat mode");
        println!("  shuffle on|off        - toggle shuffle");
        println!("  status                - show player status");
        println!("  snapshot              - print UI-friendly state snapshot");
        println!("  reload                - rescan default music dir");
        println!("  help                  - show help");
        println!("  exit                  - quit");
    }
}

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}
