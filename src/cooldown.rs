use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cooldown {
    last_emit: Option<Duration>,
}

impl Cooldown {
    pub fn ready(&self, now: Duration, window: Duration) -> bool {
        self.last_emit.is_none_or(|last| {
            now.checked_sub(last)
                .is_some_and(|elapsed| elapsed >= window)
        })
    }

    pub fn emit(&mut self, now: Duration) {
        self.last_emit = Some(now);
    }

    pub fn try_emit(&mut self, now: Duration, window: Duration) -> bool {
        if !self.ready(now, window) {
            return false;
        }
        self.emit(now);
        true
    }

    pub fn reset(&mut self) {
        self.last_emit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(15);

    fn at(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn a_fresh_cooldown_is_ready() {
        assert!(Cooldown::default().ready(at(10), WINDOW));
    }

    #[test]
    fn try_emit_blocks_a_second_bubble_inside_the_window() {
        let mut cooldown = Cooldown::default();

        assert!(cooldown.try_emit(at(10), WINDOW));
        assert!(!cooldown.try_emit(at(24), WINDOW));
    }

    #[test]
    fn a_bubble_is_allowed_at_the_window_boundary() {
        let mut cooldown = Cooldown::default();

        assert!(cooldown.try_emit(at(10), WINDOW));
        assert!(cooldown.ready(at(25), WINDOW));
        assert!(cooldown.try_emit(at(25), WINDOW));
    }

    #[test]
    fn a_bubble_is_allowed_after_the_window() {
        let mut cooldown = Cooldown::default();

        assert!(cooldown.try_emit(at(10), WINDOW));
        assert!(cooldown.try_emit(at(60), WINDOW));
        assert!(!cooldown.try_emit(at(70), WINDOW));
    }

    #[test]
    fn resetting_the_cooldown_allows_an_immediate_bubble() {
        let mut cooldown = Cooldown::default();

        assert!(cooldown.try_emit(at(10), WINDOW));
        cooldown.reset();
        assert!(cooldown.ready(at(10), WINDOW));
        assert!(cooldown.try_emit(at(10), WINDOW));
    }

    #[test]
    fn a_zero_window_never_blocks() {
        let mut cooldown = Cooldown::default();

        assert!(cooldown.try_emit(at(10), Duration::ZERO));
        assert!(cooldown.try_emit(at(10), Duration::ZERO));
    }

    #[test]
    fn a_clock_that_moves_backwards_does_not_panic() {
        let mut cooldown = Cooldown::default();

        assert!(cooldown.try_emit(at(100), WINDOW));
        assert!(!cooldown.ready(at(1), WINDOW));
        assert!(cooldown.try_emit(Duration::MAX, WINDOW));
    }
}
