use egui::{Color32, Frame, Margin, Rounding, Stroke, Visuals};

pub const ACCENT: Color32 = Color32::from_rgb(0, 111, 169);
pub const INK: Color32 = Color32::from_rgb(21, 44, 65);
pub const SURFACE: Color32 = Color32::from_rgb(236, 245, 252);
pub const SHEET: Color32 = Color32::from_rgb(252, 254, 255);
pub const BORDER: Color32 = Color32::from_rgb(207, 224, 237);
pub const VIEWPORT: [f32; 3] = [0.074, 0.149, 0.220];

pub fn apply(context: &egui::Context, dark: bool) {
    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let (panel, window, stroke, text, accent, rest, hover, active) = if dark {
        (
            Color32::from_rgb(16, 28, 42),
            Color32::from_rgb(27, 45, 61),
            Color32::from_rgb(51, 79, 101),
            Color32::from_rgb(234, 246, 253),
            Color32::from_rgb(117, 209, 255),
            Color32::from_rgb(34, 57, 76),
            Color32::from_rgb(42, 70, 93),
            Color32::from_rgb(46, 88, 116),
        )
    } else {
        (
            SURFACE,
            Color32::WHITE,
            BORDER,
            INK,
            ACCENT,
            SURFACE,
            Color32::from_rgb(221, 239, 251),
            Color32::from_rgb(205, 231, 249),
        )
    };
    visuals.panel_fill = panel;
    visuals.window_fill = window;
    visuals.window_stroke = Stroke::new(1.0_f32, stroke);
    visuals.window_rounding = Rounding::same(16.0);
    visuals.menu_rounding = Rounding::same(12.0);
    visuals.hyperlink_color = accent;
    visuals.selection.bg_fill = active;
    visuals.selection.stroke = Stroke::new(1.0_f32, accent);
    visuals.faint_bg_color = hover;
    visuals.extreme_bg_color = if dark { panel } else { SHEET };
    visuals.code_bg_color = panel;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, stroke);

    for (widget, fill) in [
        (&mut visuals.widgets.inactive, rest),
        (&mut visuals.widgets.hovered, hover),
        (&mut visuals.widgets.active, active),
        (&mut visuals.widgets.open, hover),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0_f32, stroke);
        widget.fg_stroke = Stroke::new(1.5_f32, text);
        widget.rounding = Rounding::same(9.0);
        widget.expansion = 0.0;
    }

    context.set_visuals(visuals);
    context.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

pub fn toolbar(context: &egui::Context) -> Frame {
    let style = context.style();
    let fill = style.visuals.window_fill;
    let border = style.visuals.window_stroke;
    Frame::none()
        .fill(fill)
        .stroke(border)
        .inner_margin(Margin::symmetric(8.0, 3.0))
}
