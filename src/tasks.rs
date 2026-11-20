#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: u64,
    pub text: String,
    pub done: bool,
    pub created_at_ms: u64,
    pub completed_at_ms: Option<u64>,
}

impl Task {
    pub fn new(id: u64, text: &str, created_at_ms: u64) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        Some(Self {
            id,
            text: text.to_owned(),
            done: false,
            created_at_ms,
            completed_at_ms: None,
        })
    }

    /// Returns true only when the task actually moves from open to done, so a
    /// caller can acknowledge a real completion exactly once.
    pub fn complete(&mut self, now_ms: u64) -> bool {
        if self.done {
            return false;
        }
        self.done = true;
        self.completed_at_ms = Some(now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_task_starts_open_with_no_completion_time() {
        let task = Task::new(7, "Buy milk", 1000).expect("a text task should be accepted");

        assert_eq!(task.id, 7);
        assert_eq!(task.text, "Buy milk");
        assert!(!task.done);
        assert_eq!(task.created_at_ms, 1000);
        assert_eq!(task.completed_at_ms, None);
    }

    #[test]
    fn blank_task_text_is_rejected() {
        assert!(Task::new(1, "", 0).is_none());
        assert!(Task::new(1, "   \t\n ", 0).is_none());
    }

    #[test]
    fn task_text_is_trimmed_when_stored() {
        let task = Task::new(1, "  write tests  ", 5).expect("text should be accepted");

        assert_eq!(task.text, "write tests");
    }

    #[test]
    fn completing_a_task_sets_done_and_completion_time_once() {
        let mut task = Task::new(1, "x", 10).expect("text should be accepted");

        assert!(task.complete(20));
        assert!(task.done);
        assert_eq!(task.completed_at_ms, Some(20));
        assert!(!task.complete(30), "a second call is not a transition");
        assert_eq!(task.completed_at_ms, Some(20));
    }

    #[test]
    fn an_already_done_task_cannot_be_completed_again() {
        let mut task = Task {
            id: 1,
            text: "x".to_owned(),
            done: true,
            created_at_ms: 1,
            completed_at_ms: Some(2),
        };

        assert!(!task.complete(3));
        assert_eq!(task.completed_at_ms, Some(2));
    }
}
