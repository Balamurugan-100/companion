/// Authored 5-4-3-2-1 grounding prompts. Rendering never owns this text.
pub struct Step {
    pub cue: &'static str,
    pub detail: &'static str,
}

pub const STEPS: &[Step] = &[
    Step {
        cue: "See",
        detail: "Name 5 things you can see around you.",
    },
    Step {
        cue: "Feel",
        detail: "Notice 4 things you can feel — fabric, floor, warmth.",
    },
    Step {
        cue: "Hear",
        detail: "Listen for 3 sounds, near or far.",
    },
    Step {
        cue: "Smell",
        detail: "Find 2 scents, or recall 2 you like.",
    },
    Step {
        cue: "Taste",
        detail: "Notice 1 taste, or take a slow sip of water.",
    },
];

/// Manual-only controller: the UI calls `next` and `reset`, nothing ticks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exercise {
    index: usize,
}

impl Default for Exercise {
    fn default() -> Self {
        Self { index: 0 }
    }
}

impl Exercise {
    pub fn current(&self) -> &'static Step {
        &STEPS[self.index]
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn count(&self) -> usize {
        STEPS.len()
    }

    pub fn at_end(&self) -> bool {
        self.index + 1 >= STEPS.len()
    }

    pub fn next(&mut self) {
        if !self.at_end() {
            self.index += 1;
        }
    }

    pub fn reset(&mut self) {
        self.index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_exercise_shows_the_first_authored_step() {
        let exercise = Exercise::default();

        assert_eq!(exercise.index(), 0);
        assert_eq!(exercise.current().cue, STEPS[0].cue);
        assert!(!exercise.at_end());
    }

    #[test]
    fn next_advances_one_step_at_a_time() {
        let mut exercise = Exercise::default();

        exercise.next();
        assert_eq!(exercise.index(), 1);
        assert_eq!(exercise.current().cue, STEPS[1].cue);

        exercise.next();
        assert_eq!(exercise.index(), 2);
        assert_eq!(exercise.current().cue, STEPS[2].cue);
    }

    #[test]
    fn next_stays_on_the_last_step() {
        let mut exercise = Exercise::default();

        for _ in 0..STEPS.len() + 3 {
            exercise.next();
        }

        assert!(exercise.at_end());
        assert_eq!(exercise.index(), STEPS.len() - 1);
    }

    #[test]
    fn reset_returns_to_the_first_step() {
        let mut exercise = Exercise::default();
        exercise.next();
        exercise.next();

        exercise.reset();

        assert_eq!(exercise.index(), 0);
        assert!(!exercise.at_end());
    }

    #[test]
    fn the_authored_sequence_has_five_distinct_prompts() {
        assert_eq!(STEPS.len(), 5);
        for step in STEPS {
            assert!(!step.cue.is_empty());
            assert!(!step.detail.is_empty());
        }
        for (i, step) in STEPS.iter().enumerate() {
            assert!(
                STEPS[i + 1..]
                    .iter()
                    .all(|other| other.cue != step.cue && other.detail != step.detail)
            );
        }
    }
}
