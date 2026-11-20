#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub updated_at_ms: u64,
}

impl Note {
    pub fn new(id: u64, title: &str, body: &str, updated_at_ms: u64) -> Self {
        Self {
            id,
            title: clean_title(title),
            body: body.to_owned(),
            updated_at_ms,
        }
    }

    pub fn edit(&mut self, title: &str, body: &str, now_ms: u64) {
        self.title = clean_title(title);
        self.body = body.to_owned();
        self.updated_at_ms = now_ms;
    }

    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            "Untitled note"
        } else {
            &self.title
        }
    }
}

fn clean_title(title: &str) -> String {
    title
        .trim()
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_note_keeps_its_title_multiline_body_and_timestamp() {
        let note = Note::new(3, " Trip ", "line one\nline two", 500);

        assert_eq!(note.id, 3);
        assert_eq!(note.title, "Trip");
        assert_eq!(note.body, "line one\nline two");
        assert_eq!(note.updated_at_ms, 500);
    }

    #[test]
    fn a_blank_title_has_a_readable_display_fallback() {
        let note = Note::new(1, "   ", "body", 1);

        assert_eq!(note.title, "");
        assert_eq!(note.display_title(), "Untitled note");
    }

    #[test]
    fn a_title_with_line_breaks_is_flattened_for_storage() {
        let note = Note::new(1, "first\nsecond\rthird", "", 1);

        assert_eq!(note.title, "first second third");
        assert!(!note.title.contains('\n'));
        assert!(!note.title.contains('\r'));
    }

    #[test]
    fn editing_a_note_replaces_its_content_and_refreshes_the_timestamp() {
        let mut note = Note::new(1, "before", "old", 100);

        note.edit(" after ", "new\nbody", 200);

        assert_eq!(note.title, "after");
        assert_eq!(note.body, "new\nbody");
        assert_eq!(note.updated_at_ms, 200);
    }

    #[test]
    fn editing_to_a_blank_title_restores_the_display_fallback() {
        let mut note = Note::new(1, "before", "old", 100);

        note.edit("", "", 200);

        assert_eq!(note.display_title(), "Untitled note");
        assert_eq!(note.updated_at_ms, 200);
    }
}
