// Human-friendly snapshots for debugging the current runtime state.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct QueueEntryView {
    pub queue_position: usize,
    pub library_index: usize,
    pub is_current: bool,
    pub label: String,
    pub duration_label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NowPlayingView {
    pub library_index: Option<usize>,
    pub label: String,
    pub artist: String,
    pub album: String,
    pub duration_label: String,
    pub file_path: String,
    pub position_sec: f32,
    pub paused: bool,
    pub empty: bool,
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub repeat_mode: String,
    pub shuffle_enabled: bool,
    pub tracks_scanned: usize,
    pub queue_len: usize,
    pub queue_position: Option<usize>,
    pub now_playing: NowPlayingView,
    pub queue: Vec<QueueEntryView>,
}

impl AppSnapshot {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn print_json(&self) {
        println!("{}", self.to_json());
    }
}
