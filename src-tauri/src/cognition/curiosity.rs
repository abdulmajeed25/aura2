//! Curiosity / learning-progress score (Schmidhuber-style).
//!
//! Maintains a fixed-capacity sliding window of recent free-energy values.
//! The score is the *first-half mean minus second-half mean* of the window:
//!
//! ```text
//! curiosity = mean(F_oldest_half) − mean(F_newest_half)
//! ```
//!
//! Interpretation:
//! - **Positive** — F is decreasing → the cortex is making progress (the
//!   underlying generative model is improving). Stay engaged.
//! - **Zero** — F is flat. No progress; consider exploring elsewhere.
//! - **Negative** — F is increasing → the cortex is getting more surprised.
//!   Often a sign that a reflection should fire to surface the contradiction.
//!
//! While the window is not yet full, `score()` returns 0 to avoid surfacing
//! noisy estimates from too-few samples.

use std::collections::VecDeque;

pub struct CuriosityScore {
    window: VecDeque<f32>,
    capacity: usize,
}

impl CuriosityScore {
    pub fn new(capacity: usize) -> Self {
        Self {
            window: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, f: f32) {
        if self.window.len() == self.capacity {
            self.window.pop_front();
        }
        self.window.push_back(f);
    }

    pub fn is_full(&self) -> bool {
        self.window.len() == self.capacity
    }

    pub fn len(&self) -> usize {
        self.window.len()
    }

    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Learning-progress score. Returns 0 until the window is full so a
    /// half-populated window doesn't surface a noisy estimate.
    pub fn score(&self) -> f32 {
        if !self.is_full() || self.capacity < 2 {
            return 0.0;
        }
        let half = self.capacity / 2;
        let older_sum: f32 = self.window.iter().take(half).sum();
        let newer_sum: f32 = self.window.iter().skip(self.capacity - half).sum();
        (older_sum - newer_sum) / half as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty window → score 0.
    #[test]
    fn empty_window_scores_zero() {
        let c = CuriosityScore::new(10);
        assert_eq!(c.score(), 0.0);
    }

    /// Half-populated window → still 0 (avoid noisy early estimates).
    #[test]
    fn half_full_window_still_scores_zero() {
        let mut c = CuriosityScore::new(10);
        for i in 0..5 {
            c.push(i as f32);
        }
        assert_eq!(c.score(), 0.0);
        assert!(!c.is_full());
    }

    /// Hand-computed monotone decrease:
    /// window = [10, 9, 8, 7, 6, 5, 4, 3, 2, 1] (capacity 10)
    /// first half mean = (10+9+8+7+6) / 5 = 8
    /// second half mean = (5+4+3+2+1) / 5 = 3
    /// score = 8 - 3 = 5
    #[test]
    fn monotone_decrease_scores_positive_hand_computed() {
        let mut c = CuriosityScore::new(10);
        for i in (1..=10).rev() {
            c.push(i as f32);
        }
        assert!(c.is_full());
        let s = c.score();
        assert!((s - 5.0).abs() < 1e-6, "score = {s}, expected 5.0");
    }

    /// Monotone INCREASE → score negative. Same numbers as above but
    /// reversed: window = [1, 2, ..., 10], score = -5.
    #[test]
    fn monotone_increase_scores_negative() {
        let mut c = CuriosityScore::new(10);
        for i in 1..=10 {
            c.push(i as f32);
        }
        let s = c.score();
        assert!((s + 5.0).abs() < 1e-6, "score = {s}, expected -5.0");
    }

    /// Constant F → score 0.
    #[test]
    fn constant_f_scores_zero() {
        let mut c = CuriosityScore::new(10);
        for _ in 0..10 {
            c.push(2.5);
        }
        let s = c.score();
        assert!(s.abs() < 1e-6, "score = {s}");
    }

    /// Sliding behaviour: filling beyond capacity drops the oldest values.
    #[test]
    fn pushes_beyond_capacity_drop_oldest() {
        let mut c = CuriosityScore::new(4);
        for i in 0..6 {
            c.push(i as f32);
        }
        // After 6 pushes with capacity 4, window holds [2, 3, 4, 5].
        // first half = [2, 3], mean = 2.5
        // second half = [4, 5], mean = 4.5
        // score = 2.5 - 4.5 = -2.0
        let s = c.score();
        assert!((s + 2.0).abs() < 1e-6, "score = {s}, expected -2.0");
        assert_eq!(c.len(), 4);
    }
}
