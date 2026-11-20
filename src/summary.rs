use crate::store::FocusSession;
use crate::tasks::Task;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub focus_minutes: u32,
    pub completed_tasks: usize,
}

/// Counts only completed records at or after `day_start_ms`. Session seconds are
/// summed before rounding to the nearest whole minute (half rounds up); the
/// caller owns the local-day boundary so this module stays free of clock logic.
pub fn aggregate(sessions: &[FocusSession], tasks: &[Task], day_start_ms: u64) -> Summary {
    let seconds: u64 = sessions
        .iter()
        .filter(|session| session.completed && session.started_at_ms >= day_start_ms)
        .map(|session| session.planned_seconds)
        .sum();
    let minutes = seconds.saturating_add(30) / 60;
    Summary {
        focus_minutes: u32::try_from(minutes).unwrap_or(u32::MAX),
        completed_tasks: tasks
            .iter()
            .filter(|task| task.done && task.completed_at_ms.is_some_and(|ms| ms >= day_start_ms))
            .count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_START: u64 = 10_000;

    fn session(at_ms: u64, seconds: u64, completed: bool) -> FocusSession {
        FocusSession {
            started_at_ms: at_ms,
            planned_seconds: seconds,
            completed,
        }
    }

    fn task(id: u64, completed_at_ms: Option<u64>) -> Task {
        Task {
            id,
            text: format!("task {id}"),
            done: completed_at_ms.is_some(),
            created_at_ms: completed_at_ms.unwrap_or(DAY_START).saturating_sub(1),
            completed_at_ms,
        }
    }

    #[test]
    fn empty_input_has_no_activity() {
        assert_eq!(
            aggregate(&[], &[], DAY_START),
            Summary {
                focus_minutes: 0,
                completed_tasks: 0
            }
        );
    }

    #[test]
    fn records_before_the_day_start_are_excluded() {
        let sessions = [
            session(DAY_START - 1, 1500, true),
            session(DAY_START, 1500, true),
            session(DAY_START + 5_000, 300, true),
        ];
        let tasks = [
            task(1, Some(DAY_START - 1)),
            task(2, Some(DAY_START)),
            task(3, Some(DAY_START + 5_000)),
        ];

        assert_eq!(
            aggregate(&sessions, &tasks, DAY_START),
            Summary {
                focus_minutes: 30,
                completed_tasks: 2
            }
        );
    }

    #[test]
    fn a_timestamp_exactly_at_the_day_start_is_included() {
        let sessions = [session(DAY_START, 60, true)];
        let tasks = [task(1, Some(DAY_START))];

        let summary = aggregate(&sessions, &tasks, DAY_START);

        assert_eq!(summary.focus_minutes, 1);
        assert_eq!(summary.completed_tasks, 1);
    }

    #[test]
    fn incomplete_sessions_and_unfinished_tasks_are_ignored() {
        let sessions = [
            session(DAY_START, 600, true),
            session(DAY_START + 100, 3600, false),
        ];
        let tasks = [
            task(1, Some(DAY_START)),
            task(2, None),
            Task {
                id: 3,
                text: "conflicting row".into(),
                done: true,
                created_at_ms: DAY_START,
                completed_at_ms: None,
            },
        ];

        assert_eq!(
            aggregate(&sessions, &tasks, DAY_START),
            Summary {
                focus_minutes: 10,
                completed_tasks: 1
            }
        );
    }

    #[test]
    fn partial_minutes_round_to_the_nearest_whole_minute() {
        let cases = [(89, 1), (90, 2), (149, 2), (150, 3)];

        for (seconds, expected) in cases {
            let sessions = [session(DAY_START, seconds, true)];
            assert_eq!(
                aggregate(&sessions, &[], DAY_START).focus_minutes,
                expected,
                "{seconds} seconds should round to {expected} minutes"
            );
        }
    }

    #[test]
    fn seconds_are_summed_before_rounding() {
        let sessions = [
            session(DAY_START, 50, true),
            session(DAY_START + 100, 50, true),
        ];

        assert_eq!(aggregate(&sessions, &[], DAY_START).focus_minutes, 2);
    }

    #[test]
    fn a_mixed_day_counts_both_kinds_of_activity() {
        let sessions = [
            session(DAY_START, 1500, true),
            session(DAY_START + 1, 900, true),
            session(DAY_START + 2, 900, false),
        ];
        let tasks = [
            task(1, Some(DAY_START + 10)),
            task(2, Some(DAY_START + 20)),
            task(3, None),
        ];

        assert_eq!(
            aggregate(&sessions, &tasks, DAY_START),
            Summary {
                focus_minutes: 40,
                completed_tasks: 2
            }
        );
    }
}
