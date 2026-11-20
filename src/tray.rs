use tray_icon::menu::{Menu, MenuId, MenuItem};

pub const SHOW_ID: &str = "companion.show";
pub const HIDE_ID: &str = "companion.hide";
pub const QUIT_ID: &str = "companion.quit";
pub const SETTINGS_ID: &str = "companion.settings";
pub const MOOD_ID: &str = "companion.mood";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuAction {
    Show,
    Hide,
    Quit,
    Settings,
    Mood,
}

pub fn action_for(id: &str) -> Option<MenuAction> {
    match id {
        SHOW_ID => Some(MenuAction::Show),
        HIDE_ID => Some(MenuAction::Hide),
        QUIT_ID => Some(MenuAction::Quit),
        SETTINGS_ID => Some(MenuAction::Settings),
        MOOD_ID => Some(MenuAction::Mood),
        _ => None,
    }
}

pub fn menu() -> Menu {
    let menu = Menu::new();
    menu.append(&MenuItem::with_id(
        MenuId(SHOW_ID.into()),
        "Show",
        true,
        None,
    ))
    .expect("failed to add show menu item");
    menu.append(&MenuItem::with_id(
        MenuId(HIDE_ID.into()),
        "Hide",
        true,
        None,
    ))
    .expect("failed to add hide menu item");
    menu.append(&MenuItem::with_id(
        MenuId(SETTINGS_ID.into()),
        "Settings...",
        true,
        None,
    ))
    .expect("failed to add settings menu item");
    menu.append(&MenuItem::with_id(
        MenuId(MOOD_ID.into()),
        "Mood check-in...",
        true,
        None,
    ))
    .expect("failed to add mood menu item");
    menu.append(&MenuItem::with_id(
        MenuId(QUIT_ID.into()),
        "Quit",
        true,
        None,
    ))
    .expect("failed to add quit menu item");
    menu
}

pub fn icon() -> tray_icon::Icon {
    const SIZE: usize = 32;
    const CENTER: f32 = (SIZE / 2) as f32;
    const RADIUS: f32 = 12.0;

    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - CENTER;
            let dy = y as f32 - CENTER;
            if dx * dx + dy * dy <= RADIUS * RADIUS {
                let offset = (y * SIZE + x) * 4;
                rgba[offset] = 89;
                rgba[offset + 1] = 166;
                rgba[offset + 2] = 255;
                rgba[offset + 3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE as u32, SIZE as u32).expect("failed to create tray icon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_menu_item_maps_to_show() {
        assert_eq!(action_for(SHOW_ID), Some(MenuAction::Show));
    }

    #[test]
    fn hide_menu_item_maps_to_hide() {
        assert_eq!(action_for(HIDE_ID), Some(MenuAction::Hide));
    }

    #[test]
    fn quit_menu_item_maps_to_quit() {
        assert_eq!(action_for(QUIT_ID), Some(MenuAction::Quit));
    }

    #[test]
    fn settings_menu_item_maps_to_settings() {
        assert_eq!(action_for(SETTINGS_ID), Some(MenuAction::Settings));
    }

    #[test]
    fn mood_menu_item_maps_to_mood() {
        assert_eq!(action_for(MOOD_ID), Some(MenuAction::Mood));
    }

    #[test]
    fn unknown_menu_item_is_ignored() {
        assert_eq!(action_for("companion.unknown"), None);
    }

    #[test]
    fn empty_menu_id_is_ignored() {
        assert_eq!(action_for(""), None);
    }
}
