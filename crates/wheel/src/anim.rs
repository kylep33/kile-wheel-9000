//! One clock for every transition, so the open, the hover and the petal all ease the same
//! way and all answer to the same config setting.

use std::time::Instant;

/// A transition that started at some point and runs for a caller-supplied duration.
///
/// The duration is passed in rather than stored, so a config reload changes the speed of a
/// transition already in flight instead of leaving a stale copy behind.
#[derive(Debug, Clone, Copy)]
pub struct Animation {
    started_at: Instant,
}

impl Default for Animation {
    /// Starts finished for a zero duration and starts running otherwise, which is what a
    /// freshly built wheel wants.
    fn default() -> Self {
        Self::started()
    }
}

impl Animation {
    pub fn started() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }

    pub fn restart(&mut self) {
        self.started_at = Instant::now();
    }

    /// Eased, so callers can use it as a factor directly. A zero duration means animations
    /// are off, which lands on the finished value immediately.
    pub fn progress(&self, duration_ms: u64) -> f32 {
        if duration_ms == 0 {
            return 1.0;
        }

        let elapsed_ms = self.started_at.elapsed().as_millis() as f32;
        ease_out((elapsed_ms / duration_ms as f32).clamp(0.0, 1.0))
    }

    /// Whether frames still need to be drawn for this transition.
    pub fn running(&self, duration_ms: u64) -> bool {
        self.progress(duration_ms) < 1.0
    }
}

/// Cubic ease out: fast first, settling at the end. Movement that leads the clock reads as
/// responsive, which is the point of animating at all here.
fn ease_out(progress: f32) -> f32 {
    1.0 - (1.0 - progress).powi(3)
}

/// Blends `from` towards `to`. Used instead of drawing twice with alpha, because the
/// renderer shares one brush and resetting its opacity every call is easy to get wrong.
pub fn lerp(from: f32, to: f32, factor: f32) -> f32 {
    from + (to - from) * factor.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The duration the tests use when they want the clock to still be running.
    const LONG_MS: u64 = 10_000;

    #[test]
    fn the_ends_of_the_ease_are_exact() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
    }

    #[test]
    fn the_ease_leads_rather_than_lags() {
        assert!(ease_out(0.5) > 0.5);
    }

    #[test]
    fn a_zero_duration_is_finished_before_it_starts() {
        let animation = Animation::started();

        assert_eq!(animation.progress(0), 1.0);
        assert!(!animation.running(0));
    }

    #[test]
    fn a_fresh_animation_has_barely_moved() {
        let animation = Animation::started();

        assert!(animation.progress(LONG_MS) < 0.5);
        assert!(animation.running(LONG_MS));
    }

    #[test]
    fn restarting_puts_the_clock_back_to_the_beginning() {
        let mut animation = Animation::started();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let before = animation.progress(LONG_MS);

        animation.restart();

        assert!(animation.progress(LONG_MS) <= before);
    }

    #[test]
    fn a_lerp_hits_both_ends_and_the_middle() {
        assert_eq!(lerp(2.0, 4.0, 0.0), 2.0);
        assert_eq!(lerp(2.0, 4.0, 1.0), 4.0);
        assert_eq!(lerp(2.0, 4.0, 0.5), 3.0);
    }

    #[test]
    fn a_lerp_factor_outside_the_range_is_clamped() {
        assert_eq!(lerp(2.0, 4.0, -1.0), 2.0);
        assert_eq!(lerp(2.0, 4.0, 9.0), 4.0);
    }
}
