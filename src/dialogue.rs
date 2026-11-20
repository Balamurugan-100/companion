use crate::lines;
use std::time::Duration;

pub const IDLE_THRESHOLD: Duration = Duration::from_secs(30 * 60);
pub const BUBBLE_LIFETIME: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Greeting,
    TimeChanged,
    IdleThresholdReached,
    Restored,
    MoodAnswered,
    FocusStarted,
    FocusCompleted,
    TaskCompleted,
    BreakDue,
    DevNudge,
}

impl Event {
    /// Essential events speak even in calm mode and start a fresh cooldown.
    /// A task completion is essential for the same reason as a finished focus
    /// session: the acknowledgement should not be swallowed by a recent bubble.
    /// A due break is essential too: the whole point is to make the user rest.
    pub fn bypasses_cooldown(&self) -> bool {
        matches!(
            self,
            Event::MoodAnswered | Event::FocusCompleted | Event::TaskCompleted | Event::BreakDue
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub time_of_day: TimeOfDay,
    pub idle: Duration,
    pub mood: Option<Mood>,
    pub calm_mode: bool,
    pub dev_nudge: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reaction {
    None,
    Cheer,
    Blink,
    Walk,
    Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    pub text: &'static str,
    pub suggestion: Option<&'static str>,
    pub reaction: Reaction,
}

type Rule = fn(&[Event], Context) -> Option<Line>;

const RULES: &[Rule] = &[
    mood_rule,
    focus_completed_rule,
    task_completed_rule,
    dev_nudge_rule,
    focus_started_rule,
    break_due_rule,
    restored_rule,
    idle_rule,
    greeting_rule,
];

pub fn select(events: &[Event], context: Context) -> Option<Line> {
    if context.calm_mode && !events.iter().any(Event::bypasses_cooldown) {
        return None;
    }
    RULES.iter().find_map(|rule| rule(events, context))
}

fn mood_rule(events: &[Event], context: Context) -> Option<Line> {
    if !events.contains(&Event::MoodAnswered) {
        return None;
    }
    let mood = context.mood?;
    let reply = lines::for_mood(mood);
    let reaction = match mood {
        Mood::Good => Reaction::Cheer,
        Mood::Okay => Reaction::Blink,
        Mood::Stressed => Reaction::Walk,
        Mood::Tired => Reaction::Rest,
    };
    Some(Line {
        text: reply.bubble,
        suggestion: Some(reply.suggestion),
        reaction,
    })
}

fn focus_completed_rule(events: &[Event], _context: Context) -> Option<Line> {
    events.contains(&Event::FocusCompleted).then_some(Line {
        text: lines::for_focus_completed(),
        suggestion: None,
        reaction: Reaction::Cheer,
    })
}

fn task_completed_rule(events: &[Event], _context: Context) -> Option<Line> {
    events.contains(&Event::TaskCompleted).then_some(Line {
        text: lines::for_task_completed(),
        suggestion: None,
        reaction: Reaction::Blink,
    })
}

fn dev_nudge_rule(_events: &[Event], context: Context) -> Option<Line> {
    Some(Line {
        text: context.dev_nudge?,
        suggestion: None,
        reaction: Reaction::Blink,
    })
}

fn focus_started_rule(events: &[Event], _context: Context) -> Option<Line> {
    events.contains(&Event::FocusStarted).then_some(Line {
        text: lines::for_focus_started(),
        suggestion: None,
        reaction: Reaction::Blink,
    })
}

fn break_due_rule(events: &[Event], _context: Context) -> Option<Line> {
    events.contains(&Event::BreakDue).then_some(Line {
        text: lines::for_break_due(),
        suggestion: None,
        reaction: Reaction::Rest,
    })
}

fn restored_rule(events: &[Event], _context: Context) -> Option<Line> {
    events.contains(&Event::Restored).then_some(Line {
        text: lines::for_restored(),
        suggestion: None,
        reaction: Reaction::None,
    })
}

fn idle_rule(events: &[Event], context: Context) -> Option<Line> {
    if events.contains(&Event::IdleThresholdReached) && context.idle >= IDLE_THRESHOLD {
        Some(Line {
            text: lines::for_idle(),
            suggestion: None,
            reaction: Reaction::None,
        })
    } else {
        None
    }
}

fn greeting_rule(events: &[Event], context: Context) -> Option<Line> {
    if !events.contains(&Event::Greeting) && !events.contains(&Event::TimeChanged) {
        return None;
    }
    Some(Line {
        text: lines::for_time_of_day(context.time_of_day),
        suggestion: None,
        reaction: Reaction::None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Good,
    Okay,
    Stressed,
    Tired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentBubble {
    pub text: &'static str,
    pub suggestion: Option<&'static str>,
    pub remaining: Duration,
}

impl CurrentBubble {
    pub fn new(line: Line, lifetime: Duration) -> Self {
        Self {
            text: line.text,
            suggestion: line.suggestion,
            remaining: lifetime,
        }
    }

    pub fn advance(&mut self, delta: Duration) -> bool {
        self.remaining = self.remaining.saturating_sub(delta);
        self.expired()
    }

    pub fn expired(&self) -> bool {
        self.remaining.is_zero()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdleTracker {
    elapsed: Duration,
    threshold: Duration,
    notified: bool,
}

impl IdleTracker {
    pub fn new(threshold: Duration) -> Self {
        Self {
            elapsed: Duration::ZERO,
            threshold,
            notified: false,
        }
    }

    pub fn tick(&mut self, delta: Duration) -> bool {
        self.elapsed = self.elapsed.saturating_add(delta);
        if self.notified || self.elapsed < self.threshold {
            return false;
        }
        self.notified = true;
        true
    }

    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
        self.notified = false;
    }

    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

impl Default for IdleTracker {
    fn default() -> Self {
        Self::new(IDLE_THRESHOLD)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    LateNight,
}

impl TimeOfDay {
    pub fn from_hour(hour: u32) -> Self {
        match hour {
            5..=11 => Self::Morning,
            12..=16 => Self::Afternoon,
            17..=21 => Self::Evening,
            _ => Self::LateNight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn context() -> Context {
        Context {
            time_of_day: TimeOfDay::Afternoon,
            idle: Duration::ZERO,
            mood: None,
            calm_mode: false,
            dev_nudge: None,
        }
    }

    fn sample_line() -> Line {
        Line {
            text: "A line.",
            suggestion: Some("A suggestion."),
            reaction: Reaction::None,
        }
    }

    #[test]
    fn a_new_bubble_shows_the_line_and_starts_full() {
        let bubble = CurrentBubble::new(sample_line(), BUBBLE_LIFETIME);

        assert_eq!(bubble.text, "A line.");
        assert_eq!(bubble.suggestion, Some("A suggestion."));
        assert_eq!(bubble.remaining, BUBBLE_LIFETIME);
        assert!(!bubble.expired());
    }

    #[test]
    fn a_bubble_stays_alive_until_its_remaining_time_runs_out() {
        let mut bubble = CurrentBubble::new(sample_line(), Duration::from_secs(6));

        assert!(!bubble.advance(Duration::from_secs(5)));
        assert_eq!(bubble.remaining, Duration::from_secs(1));
        assert!(!bubble.expired());
        assert!(bubble.advance(Duration::from_secs(1)));
        assert_eq!(bubble.remaining, Duration::ZERO);
        assert!(bubble.expired());
    }

    #[test]
    fn expiring_a_bubble_never_underflows_its_remaining_time() {
        let mut bubble = CurrentBubble::new(sample_line(), Duration::from_secs(2));

        assert!(!bubble.advance(Duration::from_secs(1)));
        assert!(bubble.advance(Duration::from_secs(10)));
        assert!(bubble.advance(Duration::from_secs(10)));
        assert_eq!(bubble.remaining, Duration::ZERO);
    }

    #[test]
    fn a_fresh_idle_tracker_has_not_crossed_its_threshold() {
        let tracker = IdleTracker::new(Duration::from_secs(10));
        assert_eq!(tracker.elapsed(), Duration::ZERO);
        assert_eq!(tracker, IdleTracker::new(Duration::from_secs(10)));
    }

    #[test]
    fn ticking_accumulates_time_without_firing_early() {
        let mut tracker = IdleTracker::new(Duration::from_secs(10));

        assert!(!tracker.tick(Duration::from_secs(4)));
        assert!(!tracker.tick(Duration::from_secs(5)));
        assert_eq!(tracker.elapsed(), Duration::from_secs(9));
    }

    #[test]
    fn the_idle_event_fires_once_when_the_threshold_is_crossed() {
        let mut tracker = IdleTracker::new(Duration::from_secs(10));

        assert!(!tracker.tick(Duration::from_secs(9)));
        assert!(tracker.tick(Duration::from_secs(1)));
        assert!(!tracker.tick(Duration::from_secs(5)));
        assert_eq!(tracker.elapsed(), Duration::from_secs(15));
    }

    #[test]
    fn interaction_resets_and_rearms_the_idle_tracker() {
        let mut tracker = IdleTracker::new(Duration::from_secs(10));

        assert!(tracker.tick(Duration::from_secs(12)));
        tracker.reset();
        assert_eq!(tracker.elapsed(), Duration::ZERO);
        assert!(!tracker.tick(Duration::from_secs(9)));
        assert!(tracker.tick(Duration::from_secs(1)));
    }

    #[test]
    fn a_huge_delta_saturates_instead_of_panicking() {
        let mut tracker = IdleTracker::new(Duration::from_secs(10));

        assert!(tracker.tick(Duration::MAX));
        assert_eq!(tracker.elapsed(), Duration::MAX);
    }

    #[test]
    fn hour_buckets_cover_the_day() {
        assert_eq!(TimeOfDay::from_hour(6), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_hour(14), TimeOfDay::Afternoon);
        assert_eq!(TimeOfDay::from_hour(20), TimeOfDay::Evening);
        assert_eq!(TimeOfDay::from_hour(23), TimeOfDay::LateNight);
        assert_eq!(TimeOfDay::from_hour(2), TimeOfDay::LateNight);
    }

    #[test]
    fn bucket_boundaries_are_stable() {
        assert_eq!(TimeOfDay::from_hour(5), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_hour(11), TimeOfDay::Morning);
        assert_eq!(TimeOfDay::from_hour(12), TimeOfDay::Afternoon);
        assert_eq!(TimeOfDay::from_hour(16), TimeOfDay::Afternoon);
        assert_eq!(TimeOfDay::from_hour(17), TimeOfDay::Evening);
        assert_eq!(TimeOfDay::from_hour(21), TimeOfDay::Evening);
        assert_eq!(TimeOfDay::from_hour(22), TimeOfDay::LateNight);
        assert_eq!(TimeOfDay::from_hour(4), TimeOfDay::LateNight);
    }

    #[test]
    fn a_greeting_uses_the_current_time_bucket() {
        let context = Context {
            time_of_day: TimeOfDay::LateNight,
            ..context()
        };

        let line = select(&[Event::Greeting], context).expect("a greeting should produce a line");

        assert_eq!(line.text, lines::for_time_of_day(TimeOfDay::LateNight));
        assert_eq!(line.suggestion, None);
    }

    #[test]
    fn a_time_change_reuses_the_time_of_day_rule() {
        let context = Context {
            time_of_day: TimeOfDay::Evening,
            ..context()
        };

        let line = select(&[Event::TimeChanged], context).expect("a time change should be handled");

        assert_eq!(line.text, lines::for_time_of_day(TimeOfDay::Evening));
    }

    #[test]
    fn a_restored_companion_greets_the_user_back() {
        let line = select(&[Event::Restored], context()).expect("a restore should produce a line");

        assert_eq!(line.text, lines::for_restored());
    }

    #[test]
    fn the_idle_line_needs_both_the_event_and_the_threshold() {
        let events = [Event::IdleThresholdReached];
        let too_soon = Context {
            idle: IDLE_THRESHOLD - Duration::from_secs(1),
            ..context()
        };
        let long_enough = Context {
            idle: IDLE_THRESHOLD,
            ..context()
        };

        assert!(select(&events, too_soon).is_none());
        assert_eq!(
            select(&events, long_enough)
                .expect("long idle should produce a line")
                .text,
            lines::for_idle()
        );
    }

    #[test]
    fn each_mood_gets_its_own_bubble_suggestion_and_reaction() {
        let moods = [Mood::Good, Mood::Okay, Mood::Stressed, Mood::Tired];
        let lines_for: Vec<_> = moods
            .iter()
            .map(|mood| {
                let context = Context {
                    mood: Some(*mood),
                    ..context()
                };
                select(&[Event::MoodAnswered], context).expect("a mood answer should be handled")
            })
            .collect();

        for (mood, line) in moods.iter().zip(&lines_for) {
            let reply = lines::for_mood(*mood);
            assert_eq!(line.text, reply.bubble);
            assert_eq!(line.suggestion, Some(reply.suggestion));
        }
        let reactions: HashSet<_> = lines_for.iter().map(|line| line.reaction).collect();
        assert_eq!(reactions.len(), moods.len());
        assert!(!reactions.contains(&Reaction::None));
    }

    #[test]
    fn a_mood_answer_without_a_recorded_mood_is_ignored() {
        assert!(select(&[Event::MoodAnswered], context()).is_none());
    }

    #[test]
    fn no_events_produce_no_line() {
        assert!(select(&[], context()).is_none());
    }

    #[test]
    fn a_developer_nudge_uses_the_authored_line_and_a_small_reaction() {
        let context = Context {
            dev_nudge: Some(lines::DEV_NUDGES[0]),
            ..context()
        };

        let line =
            select(&[Event::DevNudge], context).expect("a developer nudge should produce a line");
        assert_eq!(line.text, lines::DEV_NUDGES[0]);
        assert_eq!(line.reaction, Reaction::Blink);
        assert_eq!(line.suggestion, None);
    }

    #[test]
    fn a_developer_nudge_without_a_line_is_ignored() {
        assert!(select(&[Event::DevNudge], context()).is_none());
    }

    #[test]
    fn focus_events_select_their_authored_lines_and_reactions() {
        let cases = [
            (
                Event::FocusStarted,
                lines::for_focus_started(),
                Reaction::Blink,
            ),
            (
                Event::FocusCompleted,
                lines::for_focus_completed(),
                Reaction::Cheer,
            ),
            (Event::BreakDue, lines::for_break_due(), Reaction::Rest),
        ];
        for (event, expected, reaction) in cases {
            let line = select(&[event], context()).expect("a focus event should produce a line");
            assert_eq!(line.text, expected);
            assert_eq!(line.reaction, reaction);
        }
    }

    #[test]
    fn focus_completion_is_essential_and_bypasses_the_cooldown() {
        assert!(Event::FocusCompleted.bypasses_cooldown());
        assert!(Event::MoodAnswered.bypasses_cooldown());
        assert!(!Event::FocusStarted.bypasses_cooldown());
        assert!(Event::BreakDue.bypasses_cooldown());

        let calm = Context {
            calm_mode: true,
            ..context()
        };
        let line = select(&[Event::FocusCompleted], calm)
            .expect("a finished focus session is important even in calm mode");
        assert_eq!(line.reaction, Reaction::Cheer);
    }

    #[test]
    fn task_completion_selects_its_authored_line_with_a_small_reaction() {
        let line = select(&[Event::TaskCompleted], context())
            .expect("a completed task should produce a line");

        assert_eq!(line.text, lines::for_task_completed());
        assert_eq!(line.suggestion, None);
        assert_eq!(line.reaction, Reaction::Blink);
        assert_ne!(line.reaction, Reaction::Cheer);
    }

    #[test]
    fn task_completion_bypasses_the_cooldown_and_speaks_in_calm_mode() {
        assert!(Event::TaskCompleted.bypasses_cooldown());

        let calm = Context {
            calm_mode: true,
            ..context()
        };
        let line = select(&[Event::TaskCompleted], calm)
            .expect("a completion should be acknowledged even in calm mode");
        assert_eq!(line.text, lines::for_task_completed());

        for ambient in [
            Event::Greeting,
            Event::TimeChanged,
            Event::Restored,
            Event::IdleThresholdReached,
            Event::FocusStarted,
            Event::DevNudge,
        ] {
            assert!(select(&[ambient], calm).is_none());
        }
    }

    #[test]
    fn focus_completion_wins_when_both_completions_arrive_together() {
        let line = select(&[Event::TaskCompleted, Event::FocusCompleted], context())
            .expect("a completion should produce a line");

        assert_eq!(line.text, lines::for_focus_completed());
        assert_eq!(line.reaction, Reaction::Cheer);
    }

    #[test]
    fn calm_mode_suppresses_ambient_lines_but_keeps_mood_replies() {
        let calm = Context {
            calm_mode: true,
            ..context()
        };
        let events = [
            Event::Greeting,
            Event::TimeChanged,
            Event::Restored,
            Event::IdleThresholdReached,
            Event::FocusStarted,
            Event::DevNudge,
        ];
        for event in events {
            assert!(select(&[event], calm).is_none());
        }
        let calm_with_mood = Context {
            mood: Some(Mood::Stressed),
            ..calm
        };
        let line = select(&[Event::MoodAnswered], calm_with_mood)
            .expect("mood replies are essential even in calm mode");
        assert_eq!(line.text, lines::for_mood(Mood::Stressed).bubble);
    }

    #[test]
    fn calm_mode_allows_only_essential_events_to_speak() {
        let calm = Context {
            calm_mode: true,
            ..context()
        };

        for event in [
            Event::Greeting,
            Event::TimeChanged,
            Event::IdleThresholdReached,
            Event::Restored,
            Event::FocusStarted,
            Event::DevNudge,
        ] {
            assert!(
                select(&[event], calm).is_none(),
                "{event:?} is not essential and should be muted"
            );
        }

        let with_mood = Context {
            mood: Some(Mood::Good),
            ..calm
        };
        for (event, context) in [
            (Event::MoodAnswered, with_mood),
            (Event::FocusCompleted, calm),
            (Event::TaskCompleted, calm),
            (Event::BreakDue, calm),
        ] {
            assert!(
                select(&[event], context).is_some(),
                "{event:?} is essential and should still speak"
            );
        }
    }

    #[test]
    fn the_first_matching_rule_wins_when_events_overlap() {
        let busy = Context {
            idle: IDLE_THRESHOLD,
            mood: Some(Mood::Tired),
            ..context()
        };
        let all = [
            Event::Greeting,
            Event::TimeChanged,
            Event::IdleThresholdReached,
            Event::Restored,
            Event::MoodAnswered,
        ];
        assert_eq!(
            select(&all, busy).expect("the mood rule should win").text,
            lines::for_mood(Mood::Tired).bubble
        );
        let without_mood = [
            Event::Greeting,
            Event::IdleThresholdReached,
            Event::Restored,
        ];
        assert_eq!(
            select(&without_mood, busy)
                .expect("the restore rule should win")
                .text,
            lines::for_restored()
        );
        let without_restored = [Event::Greeting, Event::IdleThresholdReached];
        assert_eq!(
            select(&without_restored, busy)
                .expect("the idle rule should win")
                .text,
            lines::for_idle()
        );
        assert_eq!(
            select(&[Event::Greeting], busy)
                .expect("the greeting rule should win")
                .text,
            lines::for_time_of_day(busy.time_of_day)
        );
    }
}
