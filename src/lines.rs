use crate::dialogue::{Mood, TimeOfDay};

pub struct MoodReply {
    pub bubble: &'static str,
    pub suggestion: &'static str,
}

const MORNING: &str = "Morning. An easy start will do.";
const AFTERNOON: &str = "Still here. A slow breath helps.";
const EVENING: &str = "Winding down suits you.";
const LATE_NIGHT: &str = "Late one. Be kind to yourself.";
const IDLE: &str = "Quiet for a while. Stretch with me?";
const RESTORED: &str = "Welcome back. I kept your spot.";
const FOCUS_STARTED: &str = "Focus time. I'll keep you company.";
const FOCUS_COMPLETED: &str =
    "Session done — rest now. Stand up, unclench your jaw, look away from the screen.";
const TASK_COMPLETED: &str = "One done. That's progress.";
const BREAK_DUE: &str =
    "REST NOW. Shoulders down, slow breath out, drink some water. The code will wait.";
const SUMMARY_EMPTY: &str = "A quiet day so far. Whenever you're ready.";

/// Authored, offline nudges for a technical brain. No network, no AI.
pub const DEV_NUDGES: &[&str] = &[
    "Tiny refactor: close one unused tab. Ship the calm.",
    "Build check: unclench your jaw. Your posture just got a hotfix.",
    "OutOfMemoryError: coffee cannot allocate rest. Stretch for 20 seconds.",
    "git status on your body: 1 file modified (your neck). Restage it.",
    "Stack overflow? Step away. The answer is on line 42 of your break.",
    "Patch note: eyes off the screen for six seconds. I'll watch the logs.",
    "TODO found in human: drink water. Priority: high.",
    "Deploying a micro-break now. Rollback not recommended.",
];

const GOOD: (&str, &str) = (
    "Glad things are going well.",
    "Savour it — maybe note one small win.",
);
const OKAY: (&str, &str) = (
    "Okay is a fine place to be.",
    "Take one slow breath and carry on.",
);
const STRESSED: (&str, &str) = (
    "That sounds like a lot.",
    "Try a slow breath with me: in, hold, out.",
);
const TIRED: (&str, &str) = (
    "You've earned a pause.",
    "Rest your shoulders for a minute.",
);

pub fn for_time_of_day(bucket: TimeOfDay) -> &'static str {
    match bucket {
        TimeOfDay::Morning => MORNING,
        TimeOfDay::Afternoon => AFTERNOON,
        TimeOfDay::Evening => EVENING,
        TimeOfDay::LateNight => LATE_NIGHT,
    }
}

pub fn for_idle() -> &'static str {
    IDLE
}

pub fn for_restored() -> &'static str {
    RESTORED
}

pub fn for_focus_started() -> &'static str {
    FOCUS_STARTED
}

pub fn for_focus_completed() -> &'static str {
    FOCUS_COMPLETED
}

pub fn for_task_completed() -> &'static str {
    TASK_COMPLETED
}

pub fn for_break_due() -> &'static str {
    BREAK_DUE
}

pub fn for_dev_nudge(index: usize) -> &'static str {
    DEV_NUDGES.get(index).copied().unwrap_or(DEV_NUDGES[0])
}

pub fn summary_empty() -> &'static str {
    SUMMARY_EMPTY
}

pub fn for_mood(mood: Mood) -> MoodReply {
    let (bubble, suggestion) = match mood {
        Mood::Good => GOOD,
        Mood::Okay => OKAY,
        Mood::Stressed => STRESSED,
        Mood::Tired => TIRED,
    };
    MoodReply { bubble, suggestion }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_time_of_day_has_its_own_line() {
        let lines = [
            for_time_of_day(TimeOfDay::Morning),
            for_time_of_day(TimeOfDay::Afternoon),
            for_time_of_day(TimeOfDay::Evening),
            for_time_of_day(TimeOfDay::LateNight),
        ];
        for (i, line) in lines.iter().enumerate() {
            assert!(!line.is_empty());
            assert!(lines[i + 1..].iter().all(|other| other != line));
        }
    }

    #[test]
    fn idle_and_restored_lines_are_distinct() {
        assert_ne!(for_idle(), for_restored());
    }

    #[test]
    fn focus_lines_are_authored_and_distinct() {
        let lines = [
            for_focus_started(),
            for_focus_completed(),
            for_break_due(),
            for_idle(),
            for_restored(),
        ];
        for line in lines {
            assert!(!line.is_empty());
        }
        for (i, line) in lines.iter().enumerate() {
            assert!(lines[i + 1..].iter().all(|other| other != line));
        }
    }

    #[test]
    fn the_empty_summary_note_is_authored() {
        assert!(!summary_empty().is_empty());
    }

    #[test]
    fn rest_lines_explicitly_ask_the_user_to_rest() {
        assert!(for_break_due().contains("REST NOW"));
        assert!(for_focus_completed().contains("rest now"));
    }

    #[test]
    fn developer_nudges_are_authored_distinct_and_safe_indexed() {
        assert!(DEV_NUDGES.len() >= 6);
        for line in DEV_NUDGES {
            assert!(!line.is_empty());
        }
        for (i, line) in DEV_NUDGES.iter().enumerate() {
            assert!(DEV_NUDGES[i + 1..].iter().all(|other| other != line));
        }
        assert_eq!(for_dev_nudge(0), DEV_NUDGES[0]);
        assert_eq!(for_dev_nudge(usize::MAX), DEV_NUDGES[0]);
    }

    #[test]
    fn the_task_completion_line_is_authored_and_distinct() {
        let line = for_task_completed();

        assert!(!line.is_empty());
        for other in [
            for_focus_started(),
            for_focus_completed(),
            for_break_due(),
            for_idle(),
            for_restored(),
        ] {
            assert_ne!(line, other);
        }
    }

    #[test]
    fn each_mood_has_a_distinct_bubble_and_suggestion() {
        let replies = [
            for_mood(Mood::Good),
            for_mood(Mood::Okay),
            for_mood(Mood::Stressed),
            for_mood(Mood::Tired),
        ];
        for reply in &replies {
            assert!(!reply.bubble.is_empty());
            assert!(!reply.suggestion.is_empty());
        }
        for (i, reply) in replies.iter().enumerate() {
            assert!(
                replies[i + 1..]
                    .iter()
                    .all(|other| other.bubble != reply.bubble
                        && other.suggestion != reply.suggestion)
            );
        }
    }
}
