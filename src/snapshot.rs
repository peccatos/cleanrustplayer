#[derive(Debug, Clone)]
pub struct TrackView {
    pub library_index: usize,
    pub label: String,
    pub duration_label: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct QueueEntryView {
    pub queue_position: usize,
    pub library_index: usize,
    pub is_current: bool,
    pub label: String,
    pub duration_label: String,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
    pub fn print_pretty(&self) {
        println!("Snapshot:");
        println!("  repeat_mode: {}", self.repeat_mode);
        println!("  shuffle_enabled: {}", self.shuffle_enabled);
        println!("  tracks_scanned: {}", self.tracks_scanned);
        println!("  queue_len: {}", self.queue_len);
        println!("  queue_position: {:?}", self.queue_position);
        println!("  now_playing.label: {}", self.now_playing.label);
        println!("  now_playing.artist: {}", self.now_playing.artist);
        println!("  now_playing.album: {}", self.now_playing.album);
        println!("  now_playing.duration: {}", self.now_playing.duration_label);
        println!("  now_playing.file_path: {}", self.now_playing.file_path);
        println!("  now_playing.position_sec: {:.2}", self.now_playing.position_sec);
        println!("  now_playing.paused: {}", self.now_playing.paused);
        println!("  now_playing.empty: {}", self.now_playing.empty);
        println!("  now_playing.volume: {}", self.now_playing.volume);

        if self.queue.is_empty() {
            println!("  queue: []");
            return;
        }

        println!("  queue:");
        for entry in &self.queue {
            let marker = if entry.is_current { ">" } else { " " };
            println!(
                "    {} [{:02}] track={} {} ({})",
                marker,
                entry.queue_position,
                entry.library_index,
                entry.label,
                entry.duration_label
            );
        }
    }
}