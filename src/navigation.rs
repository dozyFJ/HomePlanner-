use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Ui, vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Destination {
    Sheet,
    Catalog,
    House,
    Scene,
}

impl Destination {
    const ALL: [Self; 4] = [Self::Sheet, Self::Catalog, Self::House, Self::Scene];

    fn label(self) -> &'static str {
        match self {
            Self::Sheet => "Холст",
            Self::Catalog => "Каталог",
            Self::House => "Дом 3D",
            Self::Scene => "Сцена 3D",
        }
    }

    fn icon(self, painter: &egui::Painter, center: Pos2, stroke: Stroke) {
        let p = |x, y| center + vec2(x, y);
        let line = |a, b| painter.line_segment([a, b], stroke);
        match self {
            Self::Sheet => {
                painter.rect_stroke(Rect::from_min_max(p(-8., -8.), p(8., 8.)), 3.0, stroke);
                line(p(-4., 0.), p(4., 0.));
                line(p(0., -4.), p(0., 4.));
            }
            Self::Catalog => {
                for x in [-7., 1.] {
                    for y in [-7., 1.] {
                        painter.rect_stroke(
                            Rect::from_min_max(p(x, y), p(x + 6., y + 6.)),
                            1.2,
                            stroke,
                        );
                    }
                }
            }
            Self::House => {
                painter.add(egui::Shape::line(
                    vec![p(-9., -1.), p(0., -8.), p(9., -1.)],
                    stroke,
                ));
                painter.add(egui::Shape::line(
                    vec![p(-6., -2.), p(-6., 8.), p(6., 8.), p(6., -2.)],
                    stroke,
                ));
                painter.add(egui::Shape::line(
                    vec![p(-1.5, 8.), p(-1.5, 3.), p(1.5, 3.), p(1.5, 8.)],
                    stroke,
                ));
            }
            Self::Scene => {
                painter.add(egui::Shape::closed_line(
                    vec![
                        p(0., -9.),
                        p(8., -5.),
                        p(8., 4.),
                        p(0., 9.),
                        p(-8., 4.),
                        p(-8., -5.),
                    ],
                    stroke,
                ));
                painter.add(egui::Shape::line(
                    vec![p(-8., -5.), p(0., 0.), p(8., -5.)],
                    stroke,
                ));
                line(p(0., 0.), p(0., 9.));
            }
        }
    }
}

pub fn bar(ui: &mut Ui, active: Destination) -> Option<Destination> {
    let mut destination = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let width = ((ui.available_width() - 6.0) / 4.0).max(1.0).floor();
        for item in Destination::ALL {
            let (rect, response) = ui.allocate_exact_size(vec2(width, 48.0), Sense::click());
            let selected = item == active;
            let color = if selected {
                ui.visuals().hyperlink_color
            } else {
                ui.visuals().text_color()
            };
            let painter = ui.painter();
            if selected || response.hovered() {
                let fill = if selected {
                    ui.visuals().selection.bg_fill
                } else {
                    ui.visuals().faint_bg_color
                };
                painter.rect_filled(rect.shrink2(vec2(2.0, 1.0)), 12.0, fill);
            }
            item.icon(
                painter,
                Pos2::new(rect.center().x, rect.top() + 15.0),
                Stroke::new(1.65_f32, color),
            );
            painter.text(
                Pos2::new(rect.center().x, rect.bottom() - 12.0),
                Align2::CENTER_CENTER,
                item.label(),
                FontId::proportional(11.5),
                color,
            );
            if response.on_hover_text(item.label()).clicked() {
                destination = Some(item);
            }
        }
    });
    destination
}
