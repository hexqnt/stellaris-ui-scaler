use eframe::egui::{self, Color32, Stroke, TextStyle, Vec2};

pub(super) const BACKGROUND: Color32 = Color32::from_rgb(25, 29, 31);
pub(super) const PANEL: Color32 = Color32::from_rgb(32, 37, 40);
pub(super) const SURFACE: Color32 = Color32::from_rgb(39, 45, 48);
pub(super) const BORDER: Color32 = Color32::from_rgb(64, 74, 78);
pub(super) const MINT: Color32 = Color32::from_rgb(155, 225, 192);
pub(super) const INK: Color32 = Color32::from_rgb(18, 42, 33);
pub(super) const MUTED: Color32 = Color32::from_rgb(157, 171, 176);

pub(super) fn install(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style
        .text_styles
        .insert(TextStyle::Body, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, egui::FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Heading, egui::FontId::proportional(21.0));
    style
        .text_styles
        .insert(TextStyle::Small, egui::FontId::proportional(12.0));
    style.spacing.item_spacing = Vec2::new(8.0, 9.0);
    style.spacing.button_padding = Vec2::new(11.0, 7.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.combo_width = 140.0;
    style.visuals.panel_fill = PANEL;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = BACKGROUND;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.weak_text_color = Some(MUTED);
    style.visuals.selection.bg_fill = MINT;
    style.visuals.selection.stroke = Stroke::new(1.0, INK);
    style.visuals.hyperlink_color = MINT;
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(5);
        widget.bg_stroke = Stroke::new(1.0, BORDER);
        widget.fg_stroke = Stroke::new(1.0, Color32::from_rgb(229, 235, 233));
    }
    style.visuals.widgets.inactive.bg_fill = SURFACE;
    style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(53, 67, 65);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(53, 67, 65);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(66, 94, 82);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.fg_stroke =
        Stroke::new(1.0, Color32::from_rgb(229, 235, 233));
    ctx.set_style_of(egui::Theme::Dark, style);
}

pub(super) fn frame(fill: Color32, margin: i8) -> egui::Frame {
    egui::Frame::new().fill(fill).inner_margin(margin)
}

pub(super) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(5.0);
    ui.strong(title);
    ui.add_space(2.0);
}

pub(super) fn primary(icon: &str, label: &str) -> egui::Button<'static> {
    egui::Button::new((
        egui::RichText::new(icon).size(17.0).color(INK),
        egui::RichText::new(label).strong().color(INK),
    ))
    .gap(7.0)
    .fill(MINT)
    .stroke(Stroke::NONE)
    .min_size(Vec2::new(128.0, 34.0))
}
