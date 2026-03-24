use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use lofty::file::AudioFile;
use lofty::file::TaggedFileExt;
use lofty::prelude::Accessor;
use lofty::probe::Probe;

pub const DEFAULT_MUSIC_DIR: &str = "C:\\Users\\burav\\Music";

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    pub file_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<Duration>,
}

impl Track {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        validate_audio_path(&path)?;

        let file_name = file_name_or_unknown(&path);
        let metadata = read_track_metadata(&path);

        Ok(Self {
            path,
            file_name,
            title: metadata.title,
            artist: metadata.artist,
            album: metadata.album,
            duration: metadata.duration,
        })
    }

    pub fn display_label(&self) -> String {
        match (self.artist.as_deref(), self.title.as_deref()) {
            (Some(artist), Some(title)) => format!("{artist} - {title}"),
            (_, Some(title)) => title.to_string(),
            _ => self.file_name.clone(),
        }
    }

    pub fn duration_label(&self) -> String {
        self.duration
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string())
    }
}

#[derive(Debug, Default)]
struct TrackMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<Duration>,
}

fn validate_audio_path(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("audio file does not exist: {}", path.display());
    }

    if !path.is_file() {
        anyhow::bail!("path is not a file: {}", path.display());
    }

    if !is_supported_audio_file(path) {
        anyhow::bail!("only mp3 is supported for now: {}", path.display());
    }

    Ok(())
}

fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("mp3"))
        .unwrap_or(false)
}

fn file_name_or_unknown(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn normalize_tag_value<T: AsRef<str>>(value: Option<T>) -> Option<String> {
    value.and_then(|value| {
        let value = value.as_ref().trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn read_track_metadata(path: &Path) -> TrackMetadata {
    let tagged_file = match Probe::open(path).and_then(|probe| probe.read()) {
        Ok(file) => file,
        Err(_) => return TrackMetadata::default(),
    };

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let (title, artist, album) = if let Some(tag) = tag {
        (
            normalize_tag_value(tag.title()),
            normalize_tag_value(tag.artist()),
            normalize_tag_value(tag.album()),
        )
    } else {
        (None, None, None)
    };

    let raw_duration = tagged_file.properties().duration();
    let duration = if raw_duration.as_secs() == 0 {
        None
    } else {
        Some(raw_duration)
    };

    TrackMetadata {
        title,
        artist,
        album,
        duration,
    }
}

fn default_music_dir() -> PathBuf {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|home| home.join("Music"))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MUSIC_DIR))
}

pub fn scan_music_dir(dir: &Path) -> Result<Vec<Track>> {
    let mut tracks: Vec<Track> = fs::read_dir(dir)
        .with_context(|| format!("failed to read music dir: {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_supported_audio_file(path))
        .map(Track::from_path)
        .collect::<Result<Vec<_>>>()?;

    tracks.sort_by(|a, b| {
        a.artist
            .as_deref()
            .unwrap_or("")
            .cmp(b.artist.as_deref().unwrap_or(""))
            .then_with(|| {
                a.title
                    .as_deref()
                    .unwrap_or("")
                    .cmp(b.title.as_deref().unwrap_or(""))
            })
            .then_with(|| a.file_name.cmp(&b.file_name))
    });

    Ok(tracks)
}

pub fn load_default_library() -> Result<Vec<Track>> {
    let music_dir = default_music_dir();
    scan_music_dir(&music_dir)
}

pub fn resolve_initial_track(arg: Option<String>, tracks: &[Track]) -> Result<Track> {
    if let Some(path_str) = arg {
        return Track::from_path(PathBuf::from(path_str));
    }

    tracks
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no mp3 files in {}", default_music_dir().display()))
}

pub fn print_tracks(tracks: &[Track]) {
    if tracks.is_empty() {
        println!("Track list is empty");
        return;
    }

    println!("Tracks:");
    for (index, track) in tracks.iter().enumerate() {
        println!(
            "  [{}] {} ({})",
            index,
            track.display_label(),
            track.duration_label()
        );
    }
}

pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    format!("{minutes:02}:{seconds:02}")
}
