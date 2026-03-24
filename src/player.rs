use std::fs::File;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer, Source};

#[derive(Debug, Clone)]
enum CurrentSource {
    Local(PathBuf),
    Remote { url: String, bytes: Vec<u8> },
}

pub struct AudioPlayer {
    device_sink: MixerDeviceSink,
    player: RodioPlayer,
    http: Client,
    current: Option<CurrentSource>,
    volume: f32,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let device_sink = DeviceSinkBuilder::open_default_sink()
            .context("failed to open default audio output")?;
        let player = RodioPlayer::connect_new(device_sink.mixer());
        let http = Client::builder()
            .build()
            .context("failed to build HTTP client for remote playback")?;

        Ok(Self {
            device_sink,
            player,
            http,
            current: None,
            volume: 1.0,
        })
    }

    pub fn output_config_debug(&self) -> String {
        format!("{:?}", self.device_sink.config())
    }

    pub fn load_and_play(&mut self, path: &Path) -> Result<()> {
        self.load_local_from(path, Duration::ZERO)
    }

    pub fn load_url_and_play(&mut self, url: &str) -> Result<()> {
        let normalized = url.trim();
        if normalized.is_empty() {
            anyhow::bail!("empty remote url");
        }

        let response = self
            .http
            .get(normalized)
            .send()
            .with_context(|| format!("remote playback request failed: {normalized}"))?;

        let response = response
            .error_for_status()
            .with_context(|| format!("remote playback returned error status: {normalized}"))?;

        let bytes = response
            .bytes()
            .context("failed to read remote audio bytes")?
            .to_vec();

        self.load_remote_bytes_from(normalized, bytes, Duration::ZERO)
    }

    pub fn current_path(&self) -> Option<&Path> {
        match &self.current {
            Some(CurrentSource::Local(path)) => Some(path.as_path()),
            _ => None,
        }
    }

    pub fn current_url(&self) -> Option<&str> {
        match &self.current {
            Some(CurrentSource::Remote { url, .. }) => Some(url.as_str()),
            _ => None,
        }
    }

    pub fn position(&self) -> Duration {
        self.player.get_pos()
    }

    pub fn is_paused(&self) -> bool {
        self.player.is_paused()
    }

    pub fn is_empty(&self) -> bool {
        self.player.empty()
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        self.player.set_volume(volume);
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn resume(&self) {
        self.player.play();
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.current = None;
    }

    pub fn seek_to(&mut self, seconds: u64) -> Result<()> {
        let offset = Duration::from_secs(seconds);
        let was_paused = self.player.is_paused();
        let current = self
            .current
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no current source"))?;

        if self.player.try_seek(offset).is_ok() {
            if was_paused {
                self.player.pause();
            }
            return Ok(());
        }

        match current {
            CurrentSource::Local(path) => self.load_local_from(&path, offset)?,
            CurrentSource::Remote { url, bytes } => {
                self.load_remote_bytes_from(&url, bytes, offset)?
            }
        }

        if was_paused {
            self.player.pause();
        }

        Ok(())
    }

    pub fn queue_len(&self) -> usize {
        self.player.len()
    }

    fn rebuild_player(&mut self) {
        let player = RodioPlayer::connect_new(self.device_sink.mixer());
        player.set_volume(self.volume);
        self.player = player;
    }

    fn audio_hint_from_extension(ext: &str) -> Option<&'static str> {
        match ext.to_ascii_lowercase().as_str() {
            "mp3" => Some("mp3"),
            "wav" => Some("wav"),
            "flac" => Some("flac"),
            "ogg" => Some("ogg"),
            "mp4" => Some("mp4"),
            "m4a" => Some("m4a"),
            "m4b" => Some("m4b"),
            "m4p" => Some("m4p"),
            "m4r" => Some("m4r"),
            "m4v" => Some("m4v"),
            "mov" => Some("mov"),
            _ => None,
        }
    }

    fn audio_hint_from_path(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::audio_hint_from_extension)
    }

    fn audio_hint_from_url(url: &str) -> Option<&'static str> {
        let without_query = url.split(['?', '#']).next().unwrap_or(url);
        Path::new(without_query)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::audio_hint_from_extension)
    }

    fn decode<R>(data: R, byte_len: u64, hint: Option<&str>, label: &str) -> Result<Decoder<R>>
    where
        R: Read + Seek + Send + Sync + 'static,
    {
        let mut builder = Decoder::builder()
            .with_data(data)
            .with_byte_len(byte_len)
            .with_seekable(true);

        if let Some(hint) = hint {
            builder = builder.with_hint(hint);
        }

        builder
            .build()
            .with_context(|| format!("failed to decode audio source: {label}"))
    }

    fn load_local_from(&mut self, path: &Path, offset: Duration) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("failed to open audio file: {}", path.display()))?;
        let byte_len = file
            .metadata()
            .with_context(|| format!("failed to read file metadata: {}", path.display()))?
            .len();
        let hint = Self::audio_hint_from_path(path);
        let decoder = Self::decode(file, byte_len, hint, &path.display().to_string())?;

        self.rebuild_player();

        if offset.is_zero() {
            self.player.append(decoder);
        } else {
            self.player.append(decoder.skip_duration(offset));
        }

        self.current = Some(CurrentSource::Local(path.to_path_buf()));
        Ok(())
    }

    fn load_remote_bytes_from(
        &mut self,
        url: &str,
        bytes: Vec<u8>,
        offset: Duration,
    ) -> Result<()> {
        if bytes.is_empty() {
            anyhow::bail!("remote audio buffer is empty");
        }

        let hint = Self::audio_hint_from_url(url);
        let byte_len = bytes.len() as u64;
        let decoder = Self::decode(Cursor::new(bytes.clone()), byte_len, hint, url)?;

        self.rebuild_player();

        if offset.is_zero() {
            self.player.append(decoder);
        } else {
            self.player.append(decoder.skip_duration(offset));
        }

        self.current = Some(CurrentSource::Remote {
            url: url.to_string(),
            bytes,
        });

        Ok(())
    }
}
