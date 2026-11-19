use egui::{Color32, Frame, Margin, Rounding, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(0, 111, 169);
pub const INK: Color32 = Color32::from_rgb(21, 44, 65);
pub const SURFACE: Color32 = Color32::from_rgb(236, 245, 252);
pub const SHEET: Color32 = Color32::from_rgb(252, 254, 255);
pub const BORDER: Color32 = Color32::from_rgb(207, 224, 237);
pub const VIEWPORT: [f32; 3] = [0.074, 0.149, 0.220];

pub fn apply(context: &egui::Context) {
    let mut visuals = Visuals::light();
    visuals.panel_fill = SURFACE;
    visuals.window_fill = Color32::WHITE;
    visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.window_rounding = Rounding::same(16.0);
    visuals.menu_rounding = Rounding::same(12.0);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = Color32::from_rgb(211, 235, 251);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.faint_bg_color = Color32::from_rgb(221, 238, 250);
    visuals.extreme_bg_color = SHEET;
    visuals.code_bg_color = SURFACE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);

    for (widget, fill) in [
        (&mut visuals.widgets.inactive, SURFACE),
        (
            &mut visuals.widgets.hovered,
            Color32::from_rgb(221, 239, 251),
        ),
        (
            &mut visuals.widgets.active,
            Color32::from_rgb(205, 231, 249),
        ),
        (&mut visuals.widgets.open, Color32::from_rgb(217, 237, 250)),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0_f32, BORDER);
        widget.fg_stroke = Stroke::new(1.5_f32, INK);
        widget.rounding = Rounding::same(9.0);
        widget.expansion = 0.0;
    }

    context.set_visuals(visuals);
    context.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

pub fn toolbar() -> Frame {
    Frame::none()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .inner_margin(Margin::symmetric(10.0, 5.0))
}
