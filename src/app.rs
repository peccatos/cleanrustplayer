use std::convert::TryFrom;
use std::io::{self, Write};

use anyhow::Result;

use crate::command::{Command, RepeatModeArg};
use crate::context::AppContext;
use crate::contract::{PlaybackState, PlaybackStatus, ReplayCoreContract};
use crate::music::library::{format_duration, resolve_initial_track, Track};
use crate::player::AudioPlayer;
use crate::provider::ProviderKind;
use crate::provider_accounts::{ProviderAccountSummary, ProviderAccountWrite};
use crate::queue::PlaybackQueue;
use crate::service::{contract_repeat_mode, ContractRuntime, ReplayCoreService};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatMode {
    Off,
    One,
    All,
}

pub struct App {
    context: AppContext,
    player: Option<AudioPlayer>,
    current_index: Option<usize>,
    repeat_mode: RepeatMode,
    queue: PlaybackQueue,
    contract_service: ReplayCoreService,
    volume: f32,
}

impl App {
    pub fn bootstrap(initial_source: Option<String>) -> Result<Self> {
        let context = AppContext::bootstrap()?;
        let contract_service = ReplayCoreService::new()?;
        let current_index = None;
        let queue = PlaybackQueue::new(context.tracks.len(), current_index, false);

        let mut app = Self {
            context,
            player: None,
            current_index,
            repeat_mode: RepeatMode::Off,
            queue,
            contract_service,
            volume: 1.0,
        };

        if let Some(source) = initial_source {
            app.open_source(&source)?;
        }

        Ok(app)
    }

    pub fn run(&mut self) -> Result<()> {
        self.print_banner();
        self.print_help();
        let stdin = io::stdin();

        loop {
            self.autonext_if_needed()?;
            print!("{}> ", self.prompt_label());
            io::stdout().flush()?;

            let mut line = String::new();
            let bytes_read = stdin.read_line(&mut line)?;
            if bytes_read == 0 {
                println!();
                break;
            }

            let input = line.trim();
            if input.is_empty() {
                continue;
            }

            match self.execute_command(input) {
                Ok(true) => {}
                Ok(false) => break,
                Err(err) => eprintln!("error: {err}"),
            }
        }

        Ok(())
    }

    pub fn execute_command(&mut self, input: &str) -> Result<bool> {
        self.execute_parsed_command(Command::parse(input))
    }

    pub fn execute_parsed_command(&mut self, command: Command) -> Result<bool> {
        match command {
            Command::List => crate::music::library::print_tracks(&self.context.tracks),
            Command::Queue => self.print_queue(),
            Command::Find(query) => self.print_find_results(&query),
            Command::QueueFind(query) => self.print_queue_find_results(&query),
            Command::Search(query) => self.print_provider_search_results(&query),
            Command::Resolve(url) => self.print_resolved_media(&url),
            Command::Providers => self.print_provider_accounts(),
            Command::ProviderSet {
                provider_id,
                payload,
            } => self.set_provider_account(&provider_id, &payload)?,
            Command::ProviderClear(provider_id) => self.clear_provider_account(&provider_id)?,
            Command::Open(source) => self.open_source(&source)?,
            Command::PlayUrl(url) => self.play_url(&url)?,
            Command::Contract => self.print_contract()?,

            Command::Play(index) => {
                let Some(track) = self.context.tracks.get(index).cloned() else {
                    println!("track index out of range");
                    return Ok(true);
                };
                self.play_track(index, &track)?;
            }

            Command::PlayName(query) => match self.find_first_track_index(&query) {
                Some(index) => {
                    let track = self.context.tracks[index].clone();
                    self.play_track(index, &track)?;
                }
                None => println!("no track matched query: {}", query),
            },

            Command::Next => self.next_track()?,
            Command::Prev => self.prev_track()?,
            Command::Pause => {
                if let Some(player) = self.player.as_ref() {
                    player.pause();
                    println!("paused");
                } else {
                    println!("nothing loaded");
                }
            }
            Command::Resume => {
                if let Some(player) = self.player.as_ref() {
                    player.resume();
                    println!("resumed");
                } else {
                    println!("nothing loaded");
                }
            }
            Command::Stop => {
                if let Some(player) = self.player.as_mut() {
                    player.stop();
                    println!("stopped");
                } else {
                    println!("nothing loaded");
                }
            }
            Command::Volume(volume) => {
                if !(0.0..=1.0).contains(&volume) {
                    println!("volume must be 0..1");
                    return Ok(true);
                }
                self.volume = volume;
                if let Some(player) = self.player.as_mut() {
                    player.set_volume(volume);
                }
                println!("volume: {}", self.volume);
            }
            Command::Seek(seconds) => {
                if let Some(player) = self.player.as_mut() {
                    match player.seek_to(seconds) {
                        Ok(()) => println!("seeked to {}s", seconds),
                        Err(err) => println!("seek error: {}", err),
                    }
                } else {
                    println!("nothing loaded");
                }
            }
            Command::Pos => {
                let position = self
                    .player
                    .as_ref()
                    .map(|player| player.position().as_secs_f32())
                    .unwrap_or(0.0);
                println!("position: {:.2}s", position);
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
                    .set_shuffle(enabled, self.context.tracks.len(), self.current_index);
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
                if let Some(player) = self.player.as_mut() {
                    player.stop();
                }
                return Ok(false);
            }
            Command::Unknown(cmd) => println!("unknown or invalid command: {}", cmd),
        }

        Ok(true)
    }

    fn print_provider_search_results(&self, query: &str) {
        let report = self
            .context
            .search_service
            .search(crate::provider::SearchQuery::new(query, 10));

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

    fn print_provider_accounts(&self) {
        match self.context.provider_accounts_snapshot() {
            Ok(accounts) => {
                if accounts.is_empty() {
                    println!("no providers");
                    return;
                }

                println!("Providers:");
                for account in accounts {
                    self.print_provider_account(&account);
                }
            }
            Err(err) => println!("provider load error: {}", err),
        }
    }

    fn set_provider_account(&self, provider_id: &str, payload: &str) -> Result<()> {
        let input: ProviderAccountWrite = serde_json::from_str(payload)
            .map_err(|err| anyhow::anyhow!("invalid provider account payload: {err}"))?;
        let summary = self.context.upsert_provider_account(provider_id, input)?;

        println!("updated provider account: {}", summary.provider_id);
        self.print_provider_account(&summary);
        Ok(())
    }

    fn clear_provider_account(&self, provider_id: &str) -> Result<()> {
        let summary = self.context.clear_provider_account(provider_id)?;

        println!("cleared provider account: {}", summary.provider_id);
        self.print_provider_account(&summary);
        Ok(())
    }

    fn print_resolved_media(&self, url: &str) {
        let normalized = url.trim();
        if normalized.is_empty() {
            println!("empty url");
            return;
        }

        let Some(provider) = self
            .context
            .search_service
            .registry()
            .find(ProviderKind::Bandcamp)
        else {
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

    fn print_provider_account(&self, account: &ProviderAccountSummary) {
        println!("  [{}] {}", account.provider_id, account.provider_name);
        println!("      kind: {}", account.provider_kind);
        println!(
            "      enabled: {}",
            if account.enabled { "yes" } else { "no" }
        );
        println!("      status: {}", account.status);
        println!(
            "      connected: {}",
            if account.has_access_token {
                "yes"
            } else {
                "no"
            }
        );
        println!(
            "      refresh_token: {}",
            if account.has_refresh_token {
                "yes"
            } else {
                "no"
            }
        );
        println!(
            "      priority: {}",
            account
                .priority
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<none>".to_string())
        );
        println!(
            "      expires: {}",
            account
                .token_expires_at
                .map(|value| value.to_rfc3339())
                .unwrap_or_else(|| "<none>".to_string())
        );
        println!(
            "      scopes: {}",
            if account.scopes.is_empty() {
                "<none>".to_string()
            } else {
                account.scopes.join(", ")
            }
        );
        println!(
            "      capabilities: scan={} stream={} download={} sync={}",
            account.scan, account.stream, account.download, account.sync
        );
    }

    fn ensure_player(&mut self) -> Result<&mut AudioPlayer> {
        if self.player.is_none() {
            let mut player = AudioPlayer::new()?;
            player.set_volume(self.volume);
            self.player = Some(player);
        }

        Ok(self.player.as_mut().expect("player must exist after init"))
    }

    fn open_source(&mut self, source: &str) -> Result<()> {
        let normalized = source.trim();
        if normalized.is_empty() {
            println!("empty source");
            return Ok(());
        }

        if normalized.starts_with("http://") || normalized.starts_with("https://") {
            return self.play_url(normalized);
        }

        self.play_path(normalized)
    }

    fn play_path(&mut self, path: &str) -> Result<()> {
        let track = resolve_initial_track(Some(path.to_string()), &self.context.tracks)?;
        let path = track.path.clone();
        let current_index = self
            .context
            .tracks
            .iter()
            .position(|candidate| candidate.path == path);

        let player = self.ensure_player()?;
        player.load_and_play(&path)?;

        self.current_index = current_index;
        self.queue
            .rebuild(self.context.tracks.len(), self.current_index);

        println!("Loaded: {}", track.display_label());
        println!("Path: {}", track.path.display());
        println!("Duration: {}", track.duration_label());

        Ok(())
    }

    fn play_url(&mut self, url: &str) -> Result<()> {
        let normalized = url.trim();
        if normalized.is_empty() {
            println!("empty url");
            return Ok(());
        }

        self.ensure_player()?.load_url_and_play(normalized)?;
        self.current_index = None;
        self.queue
            .rebuild(self.context.tracks.len(), self.current_index);

        println!("Loaded remote stream");
        println!("URL: {}", normalized);

        Ok(())
    }

    fn reload_library(&mut self) -> Result<()> {
        self.context.reload_local_library()?;
        self.current_index = self.player.as_ref().and_then(|player| {
            player.current_path().and_then(|p| {
                self.context
                    .tracks
                    .iter()
                    .position(|track| track.path.as_path() == p)
            })
        });
        self.queue
            .rebuild(self.context.tracks.len(), self.current_index);
        println!("reloaded: {} tracks", self.context.tracks.len());
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
        println!("repeat: {}", self.repeat_mode_label());
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
        for (index, track) in self.context.tracks.iter().enumerate() {
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
            let track = &self.context.tracks[track_index];
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

        self.context
            .tracks
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

    fn snapshot(&self) -> crate::snapshot::AppSnapshot {
        let current = self.current_index.and_then(|i| self.context.tracks.get(i));

        let (file_or_url, position_sec, paused, empty) = match self.player.as_ref() {
            Some(player) => (
                player
                    .current_path()
                    .map(|p| p.display().to_string())
                    .or_else(|| player.current_url().map(|u| u.to_string()))
                    .unwrap_or_else(|| "<none>".to_string()),
                player.position().as_secs_f32(),
                player.is_paused(),
                player.is_empty(),
            ),
            None => ("<idle>".to_string(), 0.0, false, true),
        };

        let now_playing = crate::snapshot::NowPlayingView {
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
            position_sec,
            paused,
            empty,
            volume: self.volume,
        };

        let queue = self
            .queue
            .entries()
            .iter()
            .enumerate()
            .filter_map(|(queue_position, &library_index)| {
                self.context.tracks.get(library_index).map(|track| {
                    crate::snapshot::QueueEntryView {
                        queue_position,
                        library_index,
                        is_current: Some(library_index) == self.current_index,
                        label: track.display_label(),
                        duration_label: track.duration_label(),
                    }
                })
            })
            .collect();

        crate::snapshot::AppSnapshot {
            repeat_mode: self.repeat_mode_label().to_string(),
            shuffle_enabled: self.queue.is_shuffle_enabled(),
            tracks_scanned: self.context.tracks.len(),
            queue_len: self.queue.len(),
            queue_position: self.queue.position(),
            now_playing,
            queue,
        }
    }

    fn play_track(&mut self, index: usize, track: &Track) -> Result<()> {
        self.ensure_player()?.load_and_play(&track.path)?;
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

        let track = self.context.tracks[next].clone();
        self.play_track(next, &track)
    }

    fn prev_track(&mut self) -> Result<()> {
        let Some(prev) = self.compute_prev_index() else {
            println!("no previous track");
            return Ok(());
        };

        let track = self.context.tracks[prev].clone();
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
        let Some(player) = self.player.as_ref() else {
            return Ok(());
        };

        if self.current_index.is_none()
            || player.is_paused()
            || player.current_path().is_none()
            || !player.is_empty()
        {
            return Ok(());
        }

        if let Some(next) = self.compute_next_index() {
            let track = self.context.tracks[next].clone();
            self.play_track(next, &track)?;
        } else {
            if let Some(player) = self.player.as_mut() {
                player.stop();
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

    fn runtime_contract(&self) -> ContractRuntime {
        let current = self
            .current_index
            .and_then(|i| self.context.catalog.tracks.get(i));

        let (status, position_ms) = match self.player.as_ref() {
            Some(player) if player.is_empty() => (PlaybackStatus::Stopped, 0_u64),
            Some(player) if player.is_paused() => {
                let position_ms = u64::try_from(player.position().as_millis()).unwrap_or(u64::MAX);
                (PlaybackStatus::Paused, position_ms)
            }
            Some(player) => {
                let position_ms = u64::try_from(player.position().as_millis()).unwrap_or(u64::MAX);
                (PlaybackStatus::Playing, position_ms)
            }
            None => (PlaybackStatus::Stopped, 0_u64),
        };

        let playback = PlaybackState {
            status,
            current_track_id: current.map(|track| track.identity.track_id.clone()),
            current_location_id: current.and_then(|track| track.preferred_location_id.clone()),
            position_ms,
            volume: f64::from(self.volume),
            muted: false,
        };

        ContractRuntime {
            playback,
            queue_order: self.queue.entries().to_vec(),
            current_queue_index: self.queue.position(),
            repeat_mode: contract_repeat_mode(self.repeat_mode),
            shuffle_enabled: self.queue.is_shuffle_enabled(),
        }
    }

    fn contract(&self) -> ReplayCoreContract {
        self.contract_service
            .build_contract(&self.context, self.runtime_contract())
    }

    fn print_contract(&self) -> Result<()> {
        let contract = self.contract();
        self.contract_service.validate_contract(&contract)?;
        println!("{}", serde_json::to_string_pretty(&contract)?);
        Ok(())
    }

    fn print_banner(&self) {
        println!("ReplayCore CLI");
        println!("  user: {}", self.context.user_id);
        println!("  local roots: {}", self.context.local_music_roots.len());
        println!("  tracks: {}", self.context.tracks.len());
        println!("  catalog sources: {}", self.context.catalog.sources.len());
        println!("  type `help` to list commands");
    }

    fn prompt_label(&self) -> String {
        let status = match self.player.as_ref() {
            Some(player) if player.is_empty() => "idle",
            Some(player) if player.is_paused() => "paused",
            Some(_) => "playing",
            None => "idle",
        };

        let track = self
            .current_index
            .and_then(|i| self.context.tracks.get(i))
            .map(|track| track.display_label())
            .or_else(|| {
                self.player.as_ref().and_then(|player| {
                    player
                        .current_path()
                        .map(|path| {
                            path.file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string())
                        })
                        .or_else(|| player.current_url().map(|url| url.to_string()))
                })
            })
            .unwrap_or_else(|| "no-track".to_string());

        let mut label = format!("replaycore:{status}");
        if !track.is_empty() {
            let mut short_track = track;
            if short_track.len() > 30 {
                short_track.truncate(27);
                short_track.push_str("...");
            }
            label.push(' ');
            label.push('[');
            label.push_str(&short_track);
            label.push(']');
        }

        label
    }

    fn print_help(&self) {
        println!("Commands:");
        println!("  open <path|url>       - open a local file or remote url");
        println!("  list                  - show scanned tracks");
        println!("  queue                 - show playback queue");
        println!("  find <query>          - search local library");
        println!("  queuefind <query>     - search current queue");
        println!("  search <query>        - search provider layer");
        println!("  resolve <url>         - resolve provider page into preview stream");
        println!("  playurl <url>         - download remote stream and play it");
        println!("  play <index|query>    - play track by index or first text match");
        println!("  playname <query>      - play first library match");
        println!("  contract              - print ReplayCore contract JSON");
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
