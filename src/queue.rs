#[derive(Debug, Clone)]
pub struct PlaybackQueue {
    order: Vec<usize>,
    position: Option<usize>,
    shuffle_enabled: bool,
    shuffle_seed: u64,
}

impl PlaybackQueue {
    pub fn new(track_count: usize, current_index: Option<usize>, shuffle_enabled: bool) -> Self {
        let mut queue = Self {
            order: Vec::new(),
            position: None,
            shuffle_enabled,
            shuffle_seed: 0x9E37_79B9_7F4A_7C15,
        };

        queue.rebuild(track_count, current_index);
        queue
    }

    pub fn rebuild(&mut self, track_count: usize, current_index: Option<usize>) {
        self.order = (0..track_count).collect();

        if self.shuffle_enabled && track_count > 1 {
            let seed = self.shuffle_seed;
            self.order.sort_by_key(|&idx| deterministic_key(idx, seed));
        }

        self.position = current_index.and_then(|idx| self.order.iter().position(|&v| v == idx));
    }

    pub fn set_shuffle(&mut self, enabled: bool, track_count: usize, current_index: Option<usize>) {
        if self.shuffle_enabled != enabled {
            self.shuffle_enabled = enabled;
            if enabled {
                self.shuffle_seed = self.shuffle_seed.wrapping_add(0x517C_C1B7_2722_0A95);
            }
        }

        self.rebuild(track_count, current_index);
    }

    pub fn current_track_index(&self) -> Option<usize> {
        self.position.and_then(|pos| self.order.get(pos).copied())
    }

    pub fn set_current_track(&mut self, track_index: usize) {
        self.position = self.order.iter().position(|&idx| idx == track_index);
    }

    pub fn next_track_index(&self) -> Option<usize> {
        let pos = self.position?;
        self.order.get(pos + 1).copied()
    }

    pub fn prev_track_index(&self) -> Option<usize> {
        let pos = self.position?;
        pos.checked_sub(1)
            .and_then(|prev| self.order.get(prev).copied())
    }

    pub fn first_track_index(&self) -> Option<usize> {
        self.order.first().copied()
    }

    pub fn last_track_index(&self) -> Option<usize> {
        self.order.last().copied()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn is_shuffle_enabled(&self) -> bool {
        self.shuffle_enabled
    }

    pub fn entries(&self) -> &[usize] {
        &self.order
    }

    pub fn position(&self) -> Option<usize> {
        self.position
    }
}

fn deterministic_key(index: usize, seed: u64) -> u64 {
    let mut x = seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}
