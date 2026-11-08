//! The view's own motion between input events: the wheel zoom's glide,
//! whose length follows how fast the wheel turns (#1648). A few float
//! operations a frame, and a repaint asked only while it moves.

use egui::Pos2;

/// Time a glide takes to reach 90 % of its zoom when the wheel turns
/// slowly (a notch now and then), seconds.
pub const GLIDE_SLOW: f32 = 0.10;
/// The shortest glide, for the fastest wheel turns, seconds.
pub const GLIDE_FAST: f32 = 0.03;
/// Wheel speed, notches a second, up to which the glide is the slow one;
/// faster turns shorten it in proportion, down to [`GLIDE_FAST`].
pub const SLOW_NOTCHES_PER_SECOND: f32 = 8.0;
/// The longest gap between notches still read as one turn of the wheel,
/// seconds: a notch after a longer pause counts as slow.
const TURN_GAP: f32 = 0.25;

/// How long a glide takes to reach 90 % of its zoom for a wheel turning
/// `notches_per_second`.
pub fn glide_time(notches_per_second: f32) -> f32 {
    let s = notches_per_second.abs();
    if !s.is_finite() || s <= SLOW_NOTCHES_PER_SECOND {
        GLIDE_SLOW
    } else {
        (GLIDE_SLOW * SLOW_NOTCHES_PER_SECOND / s).max(GLIDE_FAST)
    }
}

/// The wheel zoom still to come and how fast it goes.
#[derive(Debug, Clone, Default)]
pub struct ZoomGlide {
    /// Natural log of the zoom factor still to apply.
    left: f32,
    /// The screen point the zoom goes about.
    anchor: Pos2,
    /// Time to reach 90 % of what is left, seconds.
    glide: f32,
    /// Input time of the last notch, seconds.
    last_notch: Option<f64>,
}

impl ZoomGlide {
    /// Wheel `notches` at input time `now`, each zooming `step` times
    /// (above 1), about `anchor`. The glide gets as long as the wheel's
    /// speed asks: the notches of this event over the time since the
    /// last one.
    pub fn add(&mut self, notches: f32, step: f32, anchor: Pos2, now: f64) {
        if notches == 0.0 || !notches.is_finite() || !(step > 0.0) || !step.is_finite() {
            return;
        }
        let gap = self
            .last_notch
            .map(|t| (now - t) as f32)
            .filter(|g| g.is_finite() && *g >= 0.0)
            .unwrap_or(TURN_GAP)
            .clamp(1.0 / 240.0, TURN_GAP);
        self.glide = glide_time(notches.abs() / gap);
        self.last_notch = Some(now);
        self.left += notches * step.ln();
        self.anchor = anchor;
    }

    /// The zoom factor for a frame `dt` seconds long (1 when nothing is
    /// left); the last bit goes at once, so a glide always ends.
    pub fn advance(&mut self, dt: f32) -> f32 {
        if self.left == 0.0 {
            return 1.0;
        }
        let glide = if self.glide > 0.0 {
            self.glide
        } else {
            GLIDE_SLOW
        };
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        let share = 1.0 - 0.1f32.powf(dt / glide);
        let mut d = self.left * share;
        if (self.left - d).abs() < 1e-4 {
            d = self.left;
        }
        self.left -= d;
        d.exp()
    }

    /// The point the zoom goes about.
    pub fn anchor(&self) -> Pos2 {
        self.anchor
    }

    /// Some zoom is still to come.
    pub fn active(&self) -> bool {
        self.left != 0.0
    }

    /// The time the glide takes to reach 90 %, seconds.
    pub fn glide(&self) -> f32 {
        self.glide
    }

    /// Drop the zoom still to come.
    pub fn stop(&mut self) {
        self.left = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faster_wheel_turns_glide_shorter() {
        assert_eq!(glide_time(0.0), GLIDE_SLOW);
        assert_eq!(glide_time(SLOW_NOTCHES_PER_SECOND), GLIDE_SLOW);
        assert!(glide_time(16.0) < GLIDE_SLOW);
        assert!(glide_time(32.0) < glide_time(16.0));
        assert_eq!(glide_time(1000.0), GLIDE_FAST);
        assert_eq!(glide_time(f32::NAN), GLIDE_SLOW);
        // A notch after a pause glides slowly; one 30 ms after the last
        // glides shorter and applies more of itself in the first frame.
        let mut g = ZoomGlide::default();
        g.add(1.0, 1.15, Pos2::ZERO, 10.0);
        assert_eq!(g.glide(), GLIDE_SLOW);
        let slow_first = g.advance(1.0 / 60.0);
        g.stop();
        g.add(1.0, 1.15, Pos2::ZERO, 10.03);
        assert!(g.glide() < GLIDE_SLOW / 2.0, "{}", g.glide());
        let fast_first = g.advance(1.0 / 60.0);
        assert!(fast_first > slow_first, "{fast_first} {slow_first}");
        // Several notches in one event after a pause: a fast turn too.
        let mut g = ZoomGlide::default();
        g.add(3.0, 1.15, Pos2::ZERO, 50.0);
        assert!(g.glide() < GLIDE_SLOW);
    }

    #[test]
    fn a_glide_adds_up_to_the_steps_and_ends() {
        let mut g = ZoomGlide::default();
        assert!(!g.active());
        assert_eq!(g.advance(0.016), 1.0);
        g.add(2.0, 1.25, Pos2::new(3.0, 4.0), 1.0);
        g.add(-1.0, 1.25, Pos2::new(5.0, 6.0), 1.2);
        assert_eq!(g.anchor(), Pos2::new(5.0, 6.0));
        let mut total = 1.0f32;
        let mut frames = 0;
        while g.active() {
            total *= g.advance(1.0 / 60.0);
            frames += 1;
            assert!(frames < 200, "the glide ends");
        }
        assert!((total - 1.25).abs() < 1e-3, "{total}");
        // Nothing for a frame of no time; nonsense is ignored.
        g.add(1.0, 1.15, Pos2::ZERO, 2.0);
        assert_eq!(g.advance(0.0), 1.0);
        g.stop();
        assert!(!g.active());
        g.add(f32::NAN, 1.15, Pos2::ZERO, 3.0);
        g.add(1.0, -2.0, Pos2::ZERO, 3.0);
        assert!(!g.active());
    }
}
