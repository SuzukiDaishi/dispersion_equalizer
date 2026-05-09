use nih_plug_egui::egui::{self, Color32, FontFamily, FontId, Style, TextStyle, Visuals};

pub fn apply(ctx: &egui::Context) {
    let mut style = Style::default();
    style.visuals = Visuals::dark();
    style.visuals.window_fill = Color32::from_rgb(15, 18, 25);
    style.visuals.panel_fill = Color32::from_rgb(15, 18, 25);
    style.visuals.extreme_bg_color = Color32::from_rgb(8, 11, 16);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(31, 36, 46);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 48, 60);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(50, 56, 68);
    style.visuals.selection.bg_fill = Color32::from_rgb(246, 200, 77);

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(10.0, FontFamily::Proportional),
        ),
    ]
    .into();

    ctx.set_style(style);
}
