//! Shared egui styling for the companion panel and desktop overlay.
//!
//! Nothing here changes application state. The goal is a calm, dark UI with
//! clear surfaces and one accent colour, without adding runtime dependencies.

use bevy_egui::egui::{self, Color32, Stroke};

pub fn bg() -> Color32 {
    Color32::from_rgb(13, 17, 23)
}

pub fn surface() -> Color32 {
    Color32::from_rgb(23, 29, 38)
}

pub fn card_hover() -> Color32 {
    Color32::from_rgb(30, 38, 49)
}

pub fn input() -> Color32 {
    Color32::from_rgb(17, 22, 29)
}

pub fn border() -> Color32 {
    Color32::from_rgb(52, 63, 78)
}

pub fn text() -> Color32 {
    Color32::from_rgb(233, 239, 246)
}

pub fn muted() -> Color32 {
    Color32::from_rgb(151, 166, 184)
}

pub fn faint() -> Color32 {
    Color32::from_rgb(103, 118, 136)
}

pub fn accent() -> Color32 {
    Color32::from_rgb(92, 214, 190)
}

pub fn accent_soft() -> Color32 {
    Color32::from_rgb(29, 78, 73)
}

pub fn violet() -> Color32 {
    Color32::from_rgb(158, 143, 255)
}

pub fn warm() -> Color32 {
    Color32::from_rgb(255, 190, 104)
}

pub fn danger() -> Color32 {
    Color32::from_rgb(255, 126, 121)
}

/// Applies the palette to one egui context. Each Bevy window owns its own
/// context, so styling the panel does not alter the companion overlay.
pub fn apply_visuals(ui: &mut egui::Ui, calm: bool) {
    let mut visuals = (*ui.visuals()).clone();
    visuals.dark_mode = true;
    visuals.window_fill = bg();
    visuals.panel_fill = bg();
    visuals.faint_bg_color = surface();
    visuals.extreme_bg_color = input();
    visuals.text_edit_bg_color = Some(input());
    visuals.code_bg_color = input();
    visuals.override_text_color = Some(if calm { muted() } else { text() });
    visuals.weak_text_color = Some(if calm { faint() } else { muted() });
    visuals.hyperlink_color = accent();
    visuals.window_corner_radius = 18.0.into();
    visuals.window_stroke = Stroke::new(1.0, border());
    visuals.selection.bg_fill = accent_soft();
    visuals.selection.stroke = Stroke::new(1.0, accent());

    let widgets = [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ];
    for widget in widgets {
        widget.bg_fill = surface();
        widget.weak_bg_fill = card_hover();
        widget.bg_stroke = Stroke::new(1.0, border());
        widget.corner_radius = 10.0.into();
        widget.fg_stroke = Stroke::new(1.0, if calm { muted() } else { text() });
    }
    visuals.widgets.hovered.weak_bg_fill = card_hover();
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent());
    visuals.widgets.active.weak_bg_fill = accent_soft();
    visuals.widgets.active.fg_stroke.color = accent();
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, violet());
    ui.ctx().set_visuals(visuals);

    let spacing = ui.spacing_mut();
    spacing.item_spacing = egui::vec2(8.0, 8.0);
    spacing.window_margin = egui::Margin::symmetric(18, 16);
    spacing.button_padding = egui::vec2(12.0, 7.0);
    spacing.slider_rail_height = 6.0;
}

/// A padded surface used for every major panel section.
pub fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(surface())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(14.0)
        .inner_margin(egui::Margin::symmetric(14, 12))
        .outer_margin(egui::Margin::symmetric(0, 4))
        .show(ui, |ui| {
            ui.vertical(content);
        });
}

/// Section label plus optional supporting line.
pub fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(egui::RichText::new(title).size(16.0).strong().color(text()));
    if !subtitle.is_empty() {
        ui.label(egui::RichText::new(subtitle).size(11.0).color(muted()));
    }
    ui.add_space(2.0);
}

/// A small coloured status chip, e.g. FOCUS, REST NOW, or SYNCED.
pub fn chip(ui: &mut egui::Ui, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgba_premultiplied(
            color.r(),
            color.g(),
            color.b(),
            42,
        ))
        .stroke(Stroke::new(1.0, color))
        .corner_radius(999.0)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(10.0).strong().color(color));
        });
}

pub fn primary_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(label).strong().color(text()))
            .fill(accent_soft())
            .stroke(Stroke::new(1.0, accent()))
            .corner_radius(10.0)
            .min_size(egui::vec2(72.0, 30.0)),
    )
    .clicked()
}

pub fn ghost_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(muted()))
            .fill(input())
            .stroke(Stroke::new(1.0, border()))
            .corner_radius(10.0)
            .min_size(egui::vec2(64.0, 30.0)),
    )
    .clicked()
}

pub fn danger_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(danger()))
            .fill(input())
            .stroke(Stroke::new(1.0, danger()))
            .corner_radius(10.0)
            .min_size(egui::vec2(64.0, 30.0)),
    )
    .clicked()
}

/// Segmented tab button. The selected tab is visibly accented.
pub fn tab_button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let fill = if selected { accent_soft() } else { input() };
    let stroke_color = if selected { accent() } else { border() };
    let text_color = if selected { accent() } else { muted() };
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(12.0)
                .strong()
                .color(text_color),
        )
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke_color))
        .corner_radius(9.0)
        .min_size(egui::vec2(58.0, 26.0)),
    )
    .clicked()
}

/// Thin progress bar used by the timer and breathing pacer.
pub fn progress(ui: &mut egui::Ui, fraction: f32, color: Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, border());
    let mut filled = rect;
    filled.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    ui.painter().rect_filled(filled, 3.0, color);
}
