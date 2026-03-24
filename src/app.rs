use std::env;
use std::io::{self, Write};

use anyhow::Result;

use crate::command::{Command, RepeatModeArg};
use crate::music::library::{
    format_duration,
    load_default_library,
    print_tracks,
    resolve_initial_track,
    Track,
};
use crate::player::AudioPlayer;
use crate::queue::PlaybackQueue;
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
}

impl App {
    pub fn bootstrap() -> Result<Self> {
        let mut args = env::args().skip(1);

        let tracks = load_default_library()?;
        let initial_track = resolve_initial_track(args.next(), &tracks)?;

        let current_index = tracks
            .iter()
            .position(|track| track.path == initial_track.path);

        let mut player = AudioPlayer::new()?;
        player.load_and_play(&initial_track.path)?;

        let queue = PlaybackQueue::new(tracks.len(), current_index, false);

        println!("Loaded: {}", initial_track.display_label());
        println!("Path: {}", initial_track.path.display());
        println!("Duration: {}", initial_track.duration_label());
        println!("Output: {}", player.output_config_debug());

        Ok(Self {
            player,
            tracks,
            current_index,
            repeat_mode: RepeatMode::Off,
            queue,
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
            Command::List => {
                print_tracks(&self.tracks);
            }

            Command::Queue => {
                self.print_queue();
            }

            Command::Find(query) => {
                self.print_find_results(&query);
            }

            Command::QueueFind(query) => {
                self.print_queue_find_results(&query);
            }

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
                None => {
                    println!("no track matched query: {}", query);
                }
            },

            Command::Next => {
                self.next_track()?;
            }

            Command::Prev => {
                self.prev_track()?;
            }

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

            Command::Seek(seconds) => {
                if self.current_index.is_none() {
                    println!("no current track");
                    return Ok(true);
                }

                match self.player.seek_to(seconds) {
                    Ok(()) => println!("seeked to {}s", seconds),
                    Err(err) => println!("seek error: {}", err),
                }
            }

            Command::Pos => {
                println!("position: {:.2}s", self.player.position().as_secs_f32());
            }

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
                    if self.queue.is_shuffle_enabled() { "on" } else { "off" }
                );
            }

            Command::Status => {
                self.print_status();
            }

            Command::Snapshot => {
                let snapshot = self.snapshot();
                snapshot.print_pretty();
            }

            Command::Reload => {
                self.reload_library()?;
            }

            Command::Help => {
                self.print_help();
            }

            Command::Exit => {
                self.player.stop();
                return Ok(false);
            }

            Command::Unknown(cmd) => {
                println!("unknown or invalid command: {}", cmd);
            }
        }

        Ok(true)
    }

    fn reload_library(&mut self) -> Result<()> {
        self.tracks = load_default_library()?;

        if let Some(current_path) = self.player.current_path() {
            self.current_index = self
                .tracks
                .iter()
                .position(|track| track.path.as_path() == current_path);
        } else {
            self.current_index = None;
        }

        self.queue.rebuild(self.tracks.len(), self.current_index);

        println!("reloaded: {} tracks", self.tracks.len());
        Ok(())
    }

    fn print_status(&self) {
        let snapshot = self.snapshot();

        println!("file: {}", snapshot.now_playing.file_path);
        println!("track: {}", snapshot.now_playing.label);
        println!("artist: {}", snapshot.now_playing.artist);
        println!("album: {}", snapshot.now_playing.album);
        println!("duration: {}", snapshot.now_playing.duration_label);
        println!("current_index: {:?}", snapshot.now_playing.library_index);
        println!("queue_position: {:?}", snapshot.queue_position);
        println!("queue_len: {}", snapshot.queue_len);
        println!("paused: {}", snapshot.now_playing.paused);
        println!("empty: {}", snapshot.now_playing.empty);
        println!("position_sec: {:.2}", snapshot.now_playing.position_sec);
        println!("volume: {}", snapshot.now_playing.volume);
        println!("repeat: {}", snapshot.repeat_mode);
        println!(
            "shuffle: {}",
            if snapshot.shuffle_enabled { "on" } else { "off" }
        );
        println!("tracks_scanned: {}", snapshot.tracks_scanned);
        println!("output: {}", self.player.output_config_debug());
    }

    fn print_queue(&self) {
        let snapshot = self.snapshot();

        if snapshot.queue.is_empty() {
            println!("Queue is empty");
            return;
        }

        println!("Queue:");
        for entry in snapshot.queue {
            let marker = if entry.is_current { ">" } else { " " };
            println!(
                "{} [{:02}] track={} {} ({})",
                marker,
                entry.queue_position,
                entry.library_index,
                entry.label,
                entry.duration_label
            );
        }
    }

    fn print_find_results(&self, query: &str) {
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            println!("empty query");
            return;
        }

        let mut found = 0usize;

        println!("Find results:");
        for (index, track) in self.tracks.iter().enumerate() {
            if self.track_matches_query(track, &normalized) {
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
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            println!("empty query");
            return;
        }

        let mut found = 0usize;

        println!("Queue find results:");
        for (pos, &track_index) in self.queue.entries().iter().enumerate() {
            let track = &self.tracks[track_index];
            if self.track_matches_query(track, &normalized) {
                let marker = if Some(track_index) == self.current_index { ">" } else { " " };
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
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            return None;
        }

        self.tracks
            .iter()
            .enumerate()
            .find(|(_, track)| self.track_matches_query(track, &normalized))
            .map(|(index, _)| index)
    }

    fn track_matches_query(&self, track: &Track, query: &str) -> bool {
        let file_name = track.file_name.to_lowercase();
        let label = track.display_label().to_lowercase();
        let title = track.title.as_deref().unwrap_or("").to_lowercase();
        let artist = track.artist.as_deref().unwrap_or("").to_lowercase();
        let album = track.album.as_deref().unwrap_or("").to_lowercase();

        file_name.contains(query)
            || label.contains(query)
            || title.contains(query)
            || artist.contains(query)
            || album.contains(query)
    }

    fn snapshot(&self) -> AppSnapshot {
        let current_track = self.current_index.and_then(|index| self.tracks.get(index));

        let now_playing = NowPlayingView {
            library_index: self.current_index,
            label: current_track
                .map(|track| track.display_label())
                .unwrap_or_else(|| "<none>".to_string()),
            artist: current_track
                .and_then(|track| track.artist.clone())
                .unwrap_or_else(|| "<none>".to_string()),
            album: current_track
                .and_then(|track| track.album.clone())
                .unwrap_or_else(|| "<none>".to_string()),
            duration_label: current_track
                .and_then(|track| track.duration)
                .map(format_duration)
                .unwrap_or_else(|| "--:--".to_string()),
            file_path: self
                .player
                .current_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<none>".to_string()),
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
        if self.current_index.is_none() {
            return Ok(());
        }

        if self.player.is_paused() {
            return Ok(());
        }

        if self.player.current_path().is_none() {
            return Ok(());
        }

        if !self.player.is_empty() {
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
        println!("  find <query>          - search library");
        println!("  queuefind <query>     - search current queue");
        println!("  play <index>          - play track by library index");
        println!("  playname <query>      - play first library match");
        println!("  next                  - play next track in queue");
        println!("  prev                  - play previous track in queue");
        println!("  pause                 - pause playback");
        println!("  resume                - resume playback");
        println!("  stop                  - stop playback");
        println!("  volume <0..1>         - set volume");
        println!("  seek <sec>            - seek in current track");
        println!("  pos                   - show current position");
        println!("  repeat off            - disable repeat");
        println!("  repeat one            - repeat current track");
        println!("  repeat all            - repeat full queue");
        println!("  shuffle on            - rebuild queue in deterministic shuffled order");
        println!("  shuffle off           - rebuild queue in linear order");
        println!("  status                - show player status");
        println!("  snapshot              - print UI-friendly state snapshot");
        println!("  reload                - rescan default music dir and rebuild queue");
        println!("  help                  - show help");
        println!("  exit                  - quit");
    }
}

fn normalize_query(query: &str) -> String {
    query.trim().to_lowercase()
}