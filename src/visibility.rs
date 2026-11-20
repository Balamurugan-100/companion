#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    Shown,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Show,
    Hide,
    Toggle,
}

impl Visibility {
    pub fn hidden(&self) -> bool {
        *self == Visibility::Hidden
    }
}

pub fn apply(current: Visibility, command: Command) -> Visibility {
    match command {
        Command::Show => Visibility::Shown,
        Command::Hide => Visibility::Hidden,
        Command::Toggle => match current {
            Visibility::Shown => Visibility::Hidden,
            Visibility::Hidden => Visibility::Shown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_restores_a_hidden_companion() {
        assert_eq!(apply(Visibility::Hidden, Command::Show), Visibility::Shown);
    }

    #[test]
    fn hide_hides_a_shown_companion() {
        assert_eq!(apply(Visibility::Shown, Command::Hide), Visibility::Hidden);
    }

    #[test]
    fn toggle_hides_a_shown_companion() {
        assert_eq!(
            apply(Visibility::Shown, Command::Toggle),
            Visibility::Hidden
        );
    }

    #[test]
    fn toggle_restores_a_hidden_companion() {
        assert_eq!(
            apply(Visibility::Hidden, Command::Toggle),
            Visibility::Shown
        );
    }

    #[test]
    fn show_is_idempotent() {
        assert_eq!(apply(Visibility::Shown, Command::Show), Visibility::Shown);
    }

    #[test]
    fn hide_is_idempotent() {
        assert_eq!(apply(Visibility::Hidden, Command::Hide), Visibility::Hidden);
    }

    #[test]
    fn two_toggles_return_to_the_original_state() {
        let once = apply(Visibility::Shown, Command::Toggle);
        assert_eq!(apply(once, Command::Toggle), Visibility::Shown);
    }
}
