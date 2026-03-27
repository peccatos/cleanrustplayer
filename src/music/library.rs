// Local filesystem import helpers for the user's owned collection.
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::Accessor;
use lofty::probe::Probe;

pub const DEFAULT_MUSIC_DIR: &str = "C:\\Users\\name\\Music";

const SUPPORTED_EXT: &[&str] = &["mp3", "flac", "wav", "m4a"];

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
            (Some(a), Some(t)) => format!("{a} - {t}"),
            (_, Some(t)) => t.to_string(),
            _ => self.file_name.clone(),
        }
    }

    pub fn duration_label(&self) -> String {
        self.duration
            .map(format_duration)
            .unwrap_or_else(|| "--:--".to_string())
    }
}

#[derive(Default)]
struct TrackMetadata {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    duration: Option<Duration>,
}

fn validate_audio_path(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("file does not exist: {}", path.display());
    }
    if !path.is_file() {
        anyhow::bail!("not a file: {}", path.display());
    }
    if !is_supported_audio_file(path) {
        anyhow::bail!("unsupported format: {}", path.display());
    }
    Ok(())
}

fn is_supported_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXT.iter().any(|ext| ext.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

fn file_name_or_unknown(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn normalize_tag<T: AsRef<str>>(value: Option<T>) -> Option<String> {
    value.and_then(|value| {
        let value = value.as_ref().trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

fn read_track_metadata(path: &Path) -> TrackMetadata {
    let tagged = match Probe::open(path).and_then(|p| p.read()) {
        Ok(f) => f,
        Err(_) => return TrackMetadata::default(),
    };

    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let (title, artist, album) = match tag {
        Some(tag) => (
            normalize_tag(tag.title()),
            normalize_tag(tag.artist()),
            normalize_tag(tag.album()),
        ),
        None => (None, None, None),
    };

    let d = tagged.properties().duration();
    let duration = (d.as_secs() > 0).then_some(d);

    TrackMetadata {
        title,
        artist,
        album,
        duration,
    }
}

pub fn scan_music_dir(dir: &Path) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();
    scan_recursive(dir, &mut tracks)?;

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

fn scan_recursive(dir: &Path, out: &mut Vec<Track>) -> Result<()> {
    let entries =
        fs::read_dir(dir).with_context(|| format!("failed to read dir: {}", dir.display()))?;

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();

        if path.is_dir() {
            // Recurse into nested folders.
            let _ = scan_recursive(&path, out);
            continue;
        }

        if !is_supported_audio_file(&path) {
            continue;
        }

        match Track::from_path(path) {
            Ok(track) => out.push(track),
            Err(_) => continue, // Skip broken files instead of failing the entire scan.
        }
    }

    Ok(())
}

pub fn default_music_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|home| home.join("Music"))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MUSIC_DIR))
}

pub fn load_music_library(dir: &Path) -> Result<Vec<Track>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    scan_music_dir(dir)
}

pub fn resolve_initial_track(arg: Option<String>, tracks: &[Track]) -> Result<Track> {
    if let Some(path) = arg {
        return Track::from_path(PathBuf::from(path));
    }

    tracks
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no tracks found"))
}

pub fn print_tracks(tracks: &[Track]) {
    if tracks.is_empty() {
        println!("Track list is empty");
        return;
    }

    println!("Tracks:");
    for (i, t) in tracks.iter().enumerate() {
        println!("  [{}] {} ({})", i, t.display_label(), t.duration_label());
    }
}

pub fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}
