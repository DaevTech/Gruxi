use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub struct SlidingTimeWindow {
    hits: VecDeque<Instant>,
    window_duration: Duration,
    window_size: u32,
}

impl SlidingTimeWindow {
    pub fn new(window_duration: Duration, window_size: u32) -> Self {
        Self {
            hits: VecDeque::with_capacity(window_size as usize),
            window_duration,
            window_size,
        }
    }

    /// Records a hit and returns whether it exceeds the window size
    pub fn hit(&mut self) -> bool {
        // Clean up and check if we can add a new hit
        self.cleanup_old_hits();
        if self.hits.len() as u32 >= self.window_size {
            // We are at limit, to we remove the oldest and add the new one
            self.hits.pop_front();
            self.hits.push_back(Instant::now());
            return true; // Exceeds window size
        }

        // Add the new hit
        let now = Instant::now();
        self.hits.push_back(now);
        false
    }

    pub fn get_hit_count(&mut self) -> u32 {
        self.cleanup_old_hits();
        self.hits.len() as u32
    }

    fn cleanup_old_hits(&mut self) {
        let now = Instant::now();
        while let Some(&hit_time) = self.hits.front() {
            if now.duration_since(hit_time) > self.window_duration {
                self.hits.pop_front();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_sliding_time_window() {
        let mut window = SlidingTimeWindow::new(Duration::from_secs(1), 3);
        assert!(!window.hit());
        assert!(!window.hit());
        assert!(!window.hit());
        assert!(window.hit()); // Exceeds window size
        assert_eq!(window.get_hit_count(), 3);
        sleep(Duration::from_secs(1));
        assert_eq!(window.get_hit_count(), 0);
        assert!(!window.hit()); // Old hits should be cleaned up
        assert_eq!(window.get_hit_count(), 1);
    }
}
