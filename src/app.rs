use std::env;
use std::io::{self, Write};

use anyhow::Result;

use crate::command::{Command, RepeatModeArg};
use crate::music::library::{
    format_duration, load_default_library, print_tracks, resolve_initial_track, Track,
};
use crate::player::AudioPlayer;

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
    shuffle_enabled: bool,
    shuffle_cursor: usize,
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

        println!("Loaded: {}", initial_track.display_label());
        println!("Path: {}", initial_track.path.display());
        println!("Duration: {}", initial_track.duration_label());
        println!("Output: {}", player.output_config_debug());

        Ok(Self {
            player,
            tracks,
            current_index,
            repeat_mode: RepeatMode::Off,
            shuffle_enabled: false,
            shuffle_cursor: current_index.unwrap_or(0),
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

            Command::Play(index) => {
                let Some(track) = self.tracks.get(index).cloned() else {
                    println!("track index out of range");
                    return Ok(true);
                };

                self.play_track(index, &track)?;
            }

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
                self.shuffle_enabled = enabled;

                if enabled {
                    self.shuffle_cursor = self.current_index.unwrap_or(0);
                }

                println!(
                    "shuffle: {}",
                    if self.shuffle_enabled { "on" } else { "off" }
                );
            }

            Command::Status => {
                self.print_status();
            }

            Command::Reload => {
                self.tracks = load_default_library()?;

                if let Some(current_path) = self.player.current_path() {
                    self.current_index = self
                        .tracks
                        .iter()
                        .position(|track| track.path.as_path() == current_path);
                } else {
                    self.current_index = None;
                }

                self.shuffle_cursor = self.current_index.unwrap_or(0);

                println!("reloaded: {} tracks", self.tracks.len());
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

    fn print_status(&self) {
        let current_file = match self.player.current_path() {
            Some(path) => path.display().to_string(),
            None => "<none>".to_string(),
        };

        let current_track = self.current_index.and_then(|index| self.tracks.get(index));

        let track_label = current_track
            .map(|track| track.display_label())
            .unwrap_or_else(|| "<none>".to_string());

        let artist = current_track
            .and_then(|track| track.artist.as_deref())
            .unwrap_or("<none>");

        let album = current_track
            .and_then(|track| track.album.as_deref())
            .unwrap_or("<none>");

        let duration = current_track
            .and_then(|track| track.duration)
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string());

        println!("file: {}", current_file);
        println!("track: {}", track_label);
        println!("artist: {}", artist);
        println!("album: {}", album);
        println!("duration: {}", duration);
        println!("current_index: {:?}", self.current_index);
        println!("paused: {}", self.player.is_paused());
        println!("empty: {}", self.player.is_empty());
        println!("queue_len: {}", self.player.queue_len());
        println!("position_sec: {:.2}", self.player.position().as_secs_f32());
        println!("volume: {}", self.player.volume());
        println!("repeat: {}", self.repeat_mode_label());
        println!(
            "shuffle: {}",
            if self.shuffle_enabled { "on" } else { "off" }
        );
        println!("tracks_scanned: {}", self.tracks.len());
        println!("output: {}", self.player.output_config_debug());
    }

    fn play_track(&mut self, index: usize, track: &Track) -> Result<()> {
        self.player.load_and_play(&track.path)?;
        self.current_index = Some(index);
        self.shuffle_cursor = index;

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

    fn compute_next_index(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }

        if self.shuffle_enabled {
            let next = if self.tracks.len() == 1 {
                0
            } else {
                (self.shuffle_cursor + 1) % self.tracks.len()
            };

            self.shuffle_cursor = next;
            return Some(next);
        }

        match self.current_index {
            Some(current) => {
                if current + 1 < self.tracks.len() {
                    Some(current + 1)
                } else {
                    match self.repeat_mode {
                        RepeatMode::Off => None,
                        RepeatMode::One => Some(current),
                        RepeatMode::All => Some(0),
                    }
                }
            }
            None => Some(0),
        }
    }

    fn compute_prev_index(&mut self) -> Option<usize> {
        if self.tracks.is_empty() {
            return None;
        }

        if self.shuffle_enabled {
            let prev = if self.shuffle_cursor == 0 {
                self.tracks.len() - 1
            } else {
                self.shuffle_cursor - 1
            };

            self.shuffle_cursor = prev;
            return Some(prev);
        }

        match self.current_index {
            Some(0) => Some(self.tracks.len() - 1),
            Some(current) => Some(current - 1),
            None => Some(self.tracks.len() - 1),
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

        match self.repeat_mode {
            RepeatMode::One => {
                let Some(index) = self.current_index else {
                    return Ok(());
                };

                let track = self.tracks[index].clone();
                self.play_track(index, &track)?;
            }
            RepeatMode::Off | RepeatMode::All => {
                if let Some(next) = self.compute_next_index() {
                    let track = self.tracks[next].clone();
                    self.play_track(next, &track)?;
                } else {
                    self.player.stop();
                }
            }
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
        println!("  play <index>          - play track by index");
        println!("  next                  - play next track");
        println!("  prev                  - play previous track");
        println!("  pause                 - pause playback");
        println!("  resume                - resume playback");
        println!("  stop                  - stop playback");
        println!("  volume <0..1>         - set volume");
        println!("  seek <sec>            - seek in current track");
        println!("  pos                   - show current position");
        println!("  repeat off            - disable repeat");
        println!("  repeat one            - repeat current track");
        println!("  repeat all            - repeat full library");
        println!("  shuffle on            - enable shuffle mode");
        println!("  shuffle off           - disable shuffle mode");
        println!("  status                - show player status");
        println!("  reload                - rescan default music dir");
        println!("  help                  - show help");
        println!("  exit                  - quit");
    }
}
