use std::{fs, path::PathBuf};

use egui::{Align2, Color32, FontId, Frame, Key, Rect, RichText, Sense, Stroke, vec2};
use glam::{Vec2, Vec3};
use wolfcraft_engine::renderer::FrameParams;

use crate::plan::{Furniture, FurnitureKind, OpeningKind, Plan, SHEET};

const PAPER: Color32 = Color32::from_rgb(246, 244, 235);
const INK: Color32 = Color32::from_rgb(39, 54, 59);
const ACCENT: Color32 = Color32::from_rgb(11, 117, 112);

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Room,
    Wall,
    Door,
    Window,
    Bed,
    Table,
    Sofa,
    Erase,
}

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Room => "Комната",
            Self::Wall => "Стена",
            Self::Door => "Дверь",
            Self::Window => "Окно",
            Self::Bed => "Кровать",
            Self::Table => "Стол",
            Self::Sofa => "Диван",
            Self::Erase => "Ластик",
        }
    }
}

pub struct Editor {
    pub plan: Plan,
    pub preview: bool,
    pub cutaway: bool,
    tool: Tool,
    start: Option<Vec2>,
    rotated: bool,
    undo: Vec<Plan>,
    path: String,
    message: String,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            plan: Plan::default(),
            preview: false,
            cutaway: false,
            tool: Tool::Room,
            start: None,
            rotated: false,
            undo: vec![],
            path: "house.json".into(),
            message: String::new(),
            yaw: 0.68,
            pitch: 0.72,
            distance: 37.0,
        }
    }
}

impl Editor {
    #[cfg(target_os = "android")]
    pub fn for_android(path: PathBuf) -> Self {
        let mut editor = Self {
            path: path.to_string_lossy().into_owned(),
            ..Self::default()
        };
        if path.is_file() {
            if let Err(error) = editor.open() {
                editor.message = error.to_string();
            }
            editor.undo.clear();
        }
        editor
    }

    fn remember(&mut self, previous: Plan) {
        if self.undo.len() == 40 {
            self.undo.remove(0);
        }
        self.undo.push(previous);
    }

    pub fn load_example(&mut self) {
        let mut plan = Plan::default();
        plan.room(Vec2::new(7.0, 4.0), Vec2::new(17.0, 12.0));
        plan.wall(Vec2::new(11.5, 4.0), Vec2::new(11.5, 12.0));
        plan.opening(OpeningKind::Door, Vec2::new(11.5, 9.0));
        plan.opening(OpeningKind::Door, Vec2::new(15.0, 12.0));
        plan.opening(OpeningKind::Window, Vec2::new(9.0, 4.0));
        plan.opening(OpeningKind::Window, Vec2::new(17.0, 7.0));
        for (at, kind) in [
            ([9.0, 6.0], FurnitureKind::Bed),
            ([14.5, 7.5], FurnitureKind::Table),
            ([14.5, 5.0], FurnitureKind::Sofa),
        ] {
            plan.furniture.push(Furniture {
                at,
                kind,
                rotated: false,
            });
        }
        let previous = std::mem::replace(&mut self.plan, plan);
        self.remember(previous);
        self.start = None;
    }

    fn open(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(self.path.trim());
        if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
            return Err("Файл проекта слишком большой".into());
        }
        let plan: Plan = serde_json::from_slice(&fs::read(path)?)?;
        if !plan.valid() {
            return Err("В файле есть недопустимые элементы".into());
        }
        let previous = std::mem::replace(&mut self.plan, plan);
        self.remember(previous);
        self.start = None;
        Ok(())
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(self.path.trim());
        fs::write(path, serde_json::to_vec_pretty(&self.plan)?)?;
        Ok(())
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.start = None;
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("WolfCraft Home Planner").strong());
                ui.separator();
                if ui.selectable_label(!self.preview, "Лист · 2D").clicked() {
                    self.preview = false;
                }
                if ui.selectable_label(self.preview, "Обзор · 3D").clicked() {
                    self.start = None;
                    self.preview = true;
                }
            });
            ui.add_space(4.0);
        });
        if !self.preview {
            egui::SidePanel::left("tools")
                .exact_width(if cfg!(target_os = "android") {
                    156.0
                } else {
                    180.0
                })
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("Инструменты");
                        for tool in [
                            Tool::Room,
                            Tool::Wall,
                            Tool::Door,
                            Tool::Window,
                            Tool::Bed,
                            Tool::Table,
                            Tool::Sofa,
                            Tool::Erase,
                        ] {
                            if ui
                                .selectable_label(self.tool == tool, tool.name())
                                .clicked()
                            {
                                self.tool = tool;
                                self.start = None;
                                self.message.clear();
                            }
                        }
                        ui.checkbox(&mut self.rotated, "Повернуть мебель");
                        ui.separator();
                        if ui
                            .add_enabled(!self.undo.is_empty(), egui::Button::new("Отменить"))
                            .clicked()
                        {
                            if let Some(plan) = self.undo.pop() {
                                self.plan = plan;
                            }
                            self.start = None;
                            self.message.clear();
                        }
                        if ui.button("Загрузить пример").clicked() {
                            self.load_example();
                        }
                        ui.separator();
                        if !cfg!(target_os = "android") {
                            ui.label("Файл проекта");
                            ui.text_edit_singleline(&mut self.path);
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Сохранить").clicked() {
                                self.message = self
                                    .save()
                                    .map(|_| "Проект сохранён".into())
                                    .unwrap_or_else(|e| e.to_string());
                            }
                            if ui.button("Открыть").clicked() {
                                self.message = self
                                    .open()
                                    .map(|_| "Проект открыт".into())
                                    .unwrap_or_else(|e| e.to_string());
                            }
                        });
                    });
                });
            egui::TopBottomPanel::bottom("help").show(ctx, |ui| {
                let hint = match self.tool {
                    Tool::Room | Tool::Wall if self.start.is_some() => {
                        "Укажите вторую точку. Esc отменяет построение."
                    }
                    Tool::Room => "Укажите два противоположных угла комнаты.",
                    Tool::Wall => "Укажите начало и конец стены.",
                    Tool::Door | Tool::Window => "Щёлкните по стене, чтобы установить проём.",
                    Tool::Erase => "Щёлкните по элементу, который хотите удалить.",
                    _ => "Щёлкните по листу, чтобы поставить мебель.",
                };
                ui.label(if self.message.is_empty() {
                    hint
                } else {
                    &self.message
                });
            });
            egui::CentralPanel::default().show(ctx, |ui| self.sheet(ui));
        } else {
            egui::TopBottomPanel::bottom("camera").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("−").clicked() {
                        self.distance = (self.distance * 1.16).min(95.0);
                    }
                    if ui.button("+").clicked() {
                        self.distance = (self.distance / 1.16).max(5.0);
                    }
                    if !cfg!(target_os = "android") {
                        ui.label("Перетаскивание — поворот · Колесо — масштаб");
                    }
                    ui.checkbox(&mut self.cutaway, "Срез стен");
                });
            });
            egui::CentralPanel::default()
                .frame(Frame::none())
                .show(ctx, |ui| {
                    let (_, response) = ui.allocate_exact_size(ui.available_size(), Sense::drag());
                    if response.dragged() {
                        let delta = response.drag_delta();
                        self.yaw -= delta.x * 0.008;
                        self.pitch = (self.pitch + delta.y * 0.008).clamp(0.20, 1.43);
                    }
                    if response.hovered() {
                        let scroll = ctx.input(|i| i.raw_scroll_delta.y);
                        self.distance = (self.distance * (-scroll * 0.0015).exp()).clamp(5.0, 95.0);
                    }
                });
        }
    }

    fn sheet(&mut self, ui: &mut egui::Ui) {
        let space = ui.available_rect_before_wrap().shrink(22.0);
        let scale = (space.width() / SHEET.x)
            .min(space.height() / SHEET.y)
            .max(1.0);
        let rect = Rect::from_center_size(space.center(), vec2(SHEET.x * scale, SHEET.y * scale));
        let response = ui.allocate_rect(rect, Sense::click());
        let painter = ui.painter().with_clip_rect(rect);
        let screen = |p: Vec2| rect.min + vec2(p.x * scale, p.y * scale);
        let position = |p: egui::Pos2| {
            Vec2::new((p.x - rect.left()) / scale, (p.y - rect.top()) / scale)
                .clamp(Vec2::ZERO, SHEET)
        };
        painter.rect_filled(rect, 0.0, PAPER);
        for x in 0..=(SHEET.x * 2.0) as usize {
            let x_m = x as f32 * 0.5;
            let stroke: f32 = if x % 2 == 0 { 0.8 } else { 0.4 };
            painter.line_segment(
                [screen(Vec2::new(x_m, 0.0)), screen(Vec2::new(x_m, SHEET.y))],
                Stroke::new(stroke, Color32::from_gray(208)),
            );
        }
        for y in 0..=(SHEET.y * 2.0) as usize {
            let y_m = y as f32 * 0.5;
            let stroke: f32 = if y % 2 == 0 { 0.8 } else { 0.4 };
            painter.line_segment(
                [screen(Vec2::new(0.0, y_m)), screen(Vec2::new(SHEET.x, y_m))],
                Stroke::new(stroke, Color32::from_gray(208)),
            );
        }
        for room in &self.plan.rooms {
            let a = Vec2::from_array(room.min);
            let b = Vec2::from_array(room.max);
            let room_rect = Rect::from_two_pos(screen(a), screen(b));
            painter.rect_filled(
                room_rect,
                0.0,
                Color32::from_rgba_unmultiplied(206, 179, 121, 66),
            );
            let size = b - a;
            if room_rect.width() > 80.0 && room_rect.height() > 46.0 {
                painter.text(
                    room_rect.center(),
                    Align2::CENTER_CENTER,
                    format!("{:.1} м²", size.x * size.y),
                    FontId::monospace(12.0),
                    INK,
                );
            }
        }
        let wall_stroke = (0.18 * scale).max(2.0);
        for wall in &self.plan.walls {
            let (a, b) = wall.points();
            painter.line_segment([screen(a), screen(b)], Stroke::new(wall_stroke, INK));
            let dir = (b - a).normalize();
            for opening in &wall.openings {
                let start = a + dir * (opening.at - opening.kind.width() / 2.0);
                let end = start + dir * opening.kind.width();
                let span = [screen(start), screen(end)];
                painter.line_segment(span, Stroke::new(wall_stroke + 1.0, PAPER));
                if opening.kind == OpeningKind::Window {
                    painter
                        .line_segment(span, Stroke::new(2.0_f32, Color32::from_rgb(31, 127, 174)));
                } else {
                    let open = screen(start + Vec2::new(-dir.y, dir.x) * opening.kind.width());
                    painter.line_segment([screen(start), open], Stroke::new(1.2_f32, ACCENT));
                }
            }
        }
        for item in &self.plan.furniture {
            let (a, b) = item.bounds();
            let bounds = Rect::from_two_pos(screen(a), screen(b));
            painter.rect_filled(bounds, 2.0, Color32::from_rgb(181, 211, 204));
            painter.rect_stroke(bounds, 2.0, Stroke::new(1.0_f32, ACCENT));
            let label = match item.kind {
                FurnitureKind::Bed => "К",
                FurnitureKind::Table => "С",
                FurnitureKind::Sofa => "Д",
            };
            painter.text(
                bounds.center(),
                Align2::CENTER_CENTER,
                label,
                FontId::monospace(11.0),
                INK,
            );
        }
        if let Some(pointer) = response.hover_pos() {
            let point = position(pointer);
            let snap = snap_point(point);
            if let Some(start) = self.start {
                if self.tool == Tool::Room {
                    painter.rect_stroke(
                        Rect::from_two_pos(screen(start), screen(snap)),
                        0.0,
                        Stroke::new(2.0_f32, ACCENT),
                    );
                } else {
                    painter
                        .line_segment([screen(start), screen(snap)], Stroke::new(2.0_f32, ACCENT));
                }
            }
            painter.circle_stroke(screen(snap), 3.0, Stroke::new(1.0_f32, ACCENT));
        }
        if response.clicked() {
            if let Some(pointer) = response.interact_pointer_pos() {
                self.apply(position(pointer));
            }
        }
        painter.text(
            rect.left_bottom() + vec2(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            "24 × 16 м · сетка 0,5 м",
            FontId::monospace(12.0),
            ACCENT,
        );
    }

    fn apply(&mut self, point: Vec2) {
        self.message.clear();
        let previous = self.plan.clone();
        let snap = snap_point(point);
        let changed = match self.tool {
            Tool::Room | Tool::Wall => {
                let Some(start) = self.start.take() else {
                    self.start = Some(snap);
                    return;
                };
                if self.tool == Tool::Room {
                    self.plan.room(start, snap)
                } else {
                    self.plan.wall(start, snap)
                }
            }
            Tool::Door | Tool::Window => {
                let kind = if self.tool == Tool::Door {
                    OpeningKind::Door
                } else {
                    OpeningKind::Window
                };
                self.plan.opening(kind, point)
            }
            Tool::Erase => self.plan.erase(point),
            tool => {
                let kind = match tool {
                    Tool::Bed => FurnitureKind::Bed,
                    Tool::Table => FurnitureKind::Table,
                    _ => FurnitureKind::Sofa,
                };
                let item = Furniture {
                    at: snap.to_array(),
                    kind,
                    rotated: self.rotated,
                };
                let (min, max) = item.bounds();
                if min.cmpge(Vec2::ZERO).all() && max.cmple(SHEET).all() {
                    self.plan.furniture.push(item);
                    true
                } else {
                    false
                }
            }
        };
        if changed {
            self.remember(previous);
        } else {
            self.message = "Элемент не помещается или здесь нечего удалить".into();
        }
    }

    pub fn camera(&self, aspect: f32, time: f32) -> FrameParams {
        let target = Vec3::new(SHEET.x * 0.5, 40.4, SHEET.y * 0.5);
        let offset = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * self.distance;
        FrameParams {
            eye: (target + offset).as_dvec3(),
            forward: -offset.normalize(),
            up: Vec3::Y,
            fov_y: 48_f32.to_radians(),
            aspect,
            sun_dir: Vec3::new(0.5, 0.9, 0.3).normalize(),
            day: 1.0,
            underwater: false,
            view_distance_blocks: 180.0,
            time,
            outline: None,
            crack: 0.0,
            viewmodel_fov_y: 0.0,
        }
    }
}

fn snap_point(point: Vec2) -> Vec2 {
    (point * 2.0).round() / 2.0
}
