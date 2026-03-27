// Local audio playback only; remote streaming paths are intentionally absent.
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer, Source};

#[derive(Debug, Clone)]
enum CurrentSource {
    Local(PathBuf),
}

pub struct AudioPlayer {
    device_sink: MixerDeviceSink,
    player: RodioPlayer,
    current: Option<CurrentSource>,
    volume: f32,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let device_sink = DeviceSinkBuilder::open_default_sink()
            .context("failed to open default audio output")?;
        let player = RodioPlayer::connect_new(device_sink.mixer());

        Ok(Self {
            device_sink,
            player,
            current: None,
            volume: 1.0,
        })
    }

    pub fn load_and_play(&mut self, path: &Path) -> Result<()> {
        self.load_local_from(path, Duration::ZERO)
    }

    pub fn current_path(&self) -> Option<&Path> {
        match &self.current {
            Some(CurrentSource::Local(path)) => Some(path.as_path()),
            None => None,
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
        }

        if was_paused {
            self.player.pause();
        }

        Ok(())
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
}
