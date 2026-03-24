use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player as RodioPlayer};

pub struct AudioPlayer {
    player: RodioPlayer,
    device_sink: MixerDeviceSink,
    current_path: Option<PathBuf>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self> {
        let device_sink = DeviceSinkBuilder::open_default_sink()
            .context("failed to open default audio output")?;
        let player = RodioPlayer::connect_new(device_sink.mixer());

        Ok(Self {
            player,
            device_sink,
            current_path: None,
        })
    }

    pub fn load_and_play(&mut self, path: &Path) -> Result<()> {
        let file = File::open(path)
            .with_context(|| format!("failed to open audio file: {}", path.display()))?;
        let decoder = Decoder::try_from(file)
            .with_context(|| format!("failed to decode: {}", path.display()))?;

        let volume = self.player.volume();
        let new_player = RodioPlayer::connect_new(self.device_sink.mixer());
        new_player.set_volume(volume);
        new_player.append(decoder);

        self.player = new_player;
        self.current_path = Some(path.to_path_buf());

        Ok(())
    }

    pub fn pause(&self) {
        self.player.pause();
    }

    pub fn resume(&self) {
        self.player.play();
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.current_path = None;
    }

    pub fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume);
    }

    pub fn volume(&self) -> f32 {
        self.player.volume()
    }

    pub fn seek_to(&self, seconds: u64) -> Result<()> {
        if self.current_path.is_none() {
            anyhow::bail!("no current track loaded");
        }

        self.player.try_seek(Duration::from_secs(seconds))?;
        Ok(())
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

    pub fn queue_len(&self) -> usize {
        self.player.len()
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_path.as_deref()
    }

    pub fn output_config_debug(&self) -> String {
        format!("{:?}", self.device_sink.config())
    }
}
