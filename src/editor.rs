use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use egui::{Align2, Color32, FontId, Frame, Key, Rect, RichText, Sense, Stroke, vec2};
use glam::{Vec2, Vec3};
use wolfcraft_engine::renderer::FrameParams;

use crate::furnishings::FurnishingAtlas;
use crate::modeling::Modeling;
use crate::plan::{FloorFinish, Furniture, FurnitureKind, OpeningKind, Plan, SHEET};

const PAPER: Color32 = Color32::from_rgb(246, 244, 235);
const INK: Color32 = Color32::from_rgb(39, 54, 59);
const ACCENT: Color32 = Color32::from_rgb(11, 117, 112);

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Room,
    Wall,
    Door,
    Window,
    Garage,
    Edit,
    Bed,
    Table,
    Sofa,
    Cabinet,
    Lamp,
    Erase,
}

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Room => "Комната",
            Self::Wall => "Стена",
            Self::Door => "Дверь",
            Self::Window => "Окно",
            Self::Garage => "Ворота",
            Self::Edit => "Правка",
            Self::Bed => "Кровать",
            Self::Table => "Стол",
            Self::Sofa => "Диван",
            Self::Cabinet => "Шкаф",
            Self::Lamp => "Торшер",
            Self::Erase => "Ластик",
        }
    }

    fn opening(self) -> Option<OpeningKind> {
        match self {
            Self::Door => Some(OpeningKind::Door),
            Self::Window => Some(OpeningKind::Window),
            Self::Garage => Some(OpeningKind::Garage),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum HouseShape {
    Square,
    Rectangle,
    LShape,
}

impl HouseShape {
    const ALL: [(Self, &'static str); 3] = [
        (Self::Square, "Квадрат · 5 × 5 м"),
        (Self::Rectangle, "Прямоугольник · 8 × 6 м"),
        (Self::LShape, "Г-образный · 8 × 8 м"),
    ];

    fn place(self, plan: &mut Plan, center: Vec2) {
        let size = match self {
            Self::Square => Vec2::splat(5.0),
            Self::Rectangle => Vec2::new(8.0, 6.0),
            Self::LShape => Vec2::splat(8.0),
        };
        let origin = snap_point(center - size * 0.5).clamp(Vec2::ZERO, SHEET - size);
        if matches!(self, Self::LShape) {
            plan.l_room(origin);
        } else {
            plan.room(origin, origin + size);
        }
    }
}

struct WallEdit {
    index: usize,
    length: f32,
    thickness: f32,
}

struct FurnitureDrag {
    index: usize,
    offset: Vec2,
    preview: Furniture,
}

pub struct Editor {
    pub open_updates: Option<Box<dyn Fn() -> Result<(), String>>>,
    pub plan: Plan,
    pub preview: bool,
    pub modeling_open: bool,
    pub modeling: Modeling,
    pub cutaway: bool,
    tool: Tool,
    start: Option<Vec2>,
    delayed_tap: Option<(Vec2, Instant)>,
    wall_drag_end: Option<Vec2>,
    shape_at: Option<Vec2>,
    wall_edit: Option<WallEdit>,
    room_edit: Option<usize>,
    furniture_selection: Option<usize>,
    furniture_drag: Option<FurnitureDrag>,
    portrait_sheet: Option<bool>,
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
            open_updates: None,
            preview: false,
            modeling_open: false,
            modeling: Modeling::default(),
            cutaway: false,
            tool: Tool::Room,
            start: None,
            delayed_tap: None,
            wall_drag_end: None,
            shape_at: None,
            wall_edit: None,
            room_edit: None,
            furniture_selection: None,
            furniture_drag: None,
            portrait_sheet: None,
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
        editor.modeling.set_path(path.with_file_name("model.json"));
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
            ([8.0, 10.5], FurnitureKind::Cabinet),
            ([16.0, 5.0], FurnitureKind::Lamp),
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

    fn flush_tap(&mut self) {
        if let Some((point, _)) = self.delayed_tap.take() {
            self.apply(point);
        }
    }

    fn cancel_mark(&mut self) {
        self.start = None;
        self.delayed_tap = None;
        self.wall_drag_end = None;
    }

    fn clear_selection(&mut self) {
        self.wall_edit = None;
        self.room_edit = None;
        self.furniture_selection = None;
        self.furniture_drag = None;
    }

    fn move_furniture(&mut self, index: usize, item: Furniture) {
        if self
            .plan
            .furniture
            .get(index)
            .is_some_and(|previous| *previous != item)
        {
            let previous = self.plan.clone();
            self.plan.furniture[index] = item;
            self.remember(previous);
        }
    }

    fn furniture_actions(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.furniture_selection else {
            return;
        };
        let Some(&item) = self.plan.furniture.get(index) else {
            return;
        };
        ui.horizontal_wrapped(|ui| {
            if ui.button("Повернуть 90°").clicked() {
                let rotated = Furniture {
                    rotated: !item.rotated,
                    ..item
                };
                self.move_furniture(index, item_at(rotated, Vec2::from_array(item.at)));
            }
            if ui.button("Копировать").clicked() {
                let center = Vec2::from_array(item.at);
                let size = item.size();
                let offsets = [Vec2::new(size.x + 0.25, 0.0), Vec2::new(0.0, size.z + 0.25)];
                let destination = offsets
                    .into_iter()
                    .flat_map(|offset| [center + offset, center - offset])
                    .map(|point| item_at(item, snap_point(point)))
                    .find(|copy| {
                        let (a, b) = copy.bounds();
                        self.plan.furniture.iter().all(|existing| {
                            let (c, d) = existing.bounds();
                            a.cmpge(d).any() || c.cmpge(b).any()
                        })
                    });
                if let Some(copy) = destination {
                    let previous = self.plan.clone();
                    self.furniture_selection = Some(self.plan.furniture.len());
                    self.plan.furniture.push(copy);
                    self.remember(previous);
                    self.message.clear();
                } else {
                    self.message = "Рядом нет места для копии".into();
                }
            }
            if ui.button("Удалить").clicked() {
                let previous = self.plan.clone();
                self.plan.furniture.remove(index);
                self.clear_selection();
                self.remember(previous);
            }
        });
    }

    fn room_editor(&mut self, ctx: &egui::Context) {
        let Some(index) = self.room_edit else { return };
        let Some(room) = self.plan.rooms.get(index) else {
            return;
        };
        let mut open = true;
        let mut finish = room.finish;
        egui::Window::new("Цвет пола")
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size.y = 34.0;
                for (option, label) in FloorFinish::ALL {
                    ui.horizontal(|ui| {
                        ui.colored_label(floor_color(option), "■");
                        if ui.selectable_label(finish == option, label).clicked() {
                            finish = option;
                        }
                    });
                }
            });
        if finish != room.finish {
            let previous = self.plan.clone();
            self.plan.rooms[index].finish = finish;
            self.remember(previous);
            self.room_edit = None;
        } else if !open {
            self.room_edit = None;
        }
    }

    fn shape_menu(&mut self, ctx: &egui::Context) {
        let Some(point) = self.shape_at else { return };
        let mut open = true;
        egui::Window::new("Готовые формы дома")
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size.y = 36.0;
                for (shape, label) in HouseShape::ALL {
                    if ui.button(label).clicked() {
                        let previous = self.plan.clone();
                        shape.place(&mut self.plan, point);
                        self.remember(previous);
                        self.shape_at = None;
                        self.message.clear();
                    }
                }
            });
        if !open {
            self.shape_at = None;
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, atlas: &FurnishingAtlas) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.cancel_mark();
            self.shape_at = None;
            self.clear_selection();
        }
        if ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Touch {
                        phase: egui::TouchPhase::Cancel,
                        ..
                    }
                )
            })
        }) {
            egui::DragAndDrop::clear_payload(ctx);
            self.cancel_mark();
            self.furniture_drag = None;
        }
        let mobile = cfg!(target_os = "android");
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            if mobile {
                ui.spacing_mut().interact_size.y = 36.0;
            }
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let title = if mobile {
                    "Home Planner"
                } else {
                    "WolfCraft Home Planner"
                };
                ui.heading(RichText::new(title).strong());
                ui.separator();
                if ui
                    .selectable_label(!self.preview && !self.modeling_open, "Лист · 2D")
                    .clicked()
                {
                    self.preview = false;
                    self.modeling_open = false;
                }
                if ui.selectable_label(self.preview, "Дом · 3D").clicked() {
                    self.modeling_open = false;
                    self.flush_tap();
                    self.cancel_mark();
                    self.shape_at = None;
                    self.clear_selection();
                    self.message.clear();
                    self.preview = true;
                }
                if ui
                    .selectable_label(self.modeling_open, "Моделирование · 3D")
                    .clicked()
                {
                    self.flush_tap();
                    self.cancel_mark();
                    self.clear_selection();
                    self.shape_at = None;
                    self.preview = false;
                    self.modeling_open = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                if !self.preview && !self.modeling_open {
                    let current = self.portrait_sheet.unwrap_or(mobile);
                    let portrait = ui.selectable_label(current, "Портрет").clicked();
                    let landscape = ui.selectable_label(!current, "Альбом").clicked();
                    if portrait || landscape {
                        self.flush_tap();
                        self.cancel_mark();
                        self.portrait_sheet = Some(portrait);
                    }
                }
                if self.open_updates.is_some() && ui.button("Обновления").clicked() {
                    self.flush_tap();
                    let saved = if self.modeling_open {
                        self.modeling.save()
                    } else {
                        self.save()
                    };
                    if let Err(error) = saved {
                        self.message = format!("Не удалось сохранить проект: {error}");
                    } else if let Some(open) = &self.open_updates {
                        if open().is_err() {
                            self.message = "Не удалось открыть обновления".into();
                        }
                    }
                }
            });
            if (self.preview || self.modeling_open) && !self.message.is_empty() {
                ui.label(&self.message);
            }
            ui.add_space(4.0);
        });
        if self.modeling_open {
            self.modeling.ui(ctx, mobile);
            return;
        }
        if !self.preview {
            if mobile {
                egui::TopBottomPanel::bottom("tools").show(ctx, |ui| self.tools(ui, true));
            } else {
                egui::SidePanel::left("tools")
                    .exact_width(180.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| self.tools(ui, false));
                    });
            }
            egui::TopBottomPanel::bottom("help").show(ctx, |ui| {
                let hint = match self.tool {
                    Tool::Wall if self.start.is_some() => "Потяните стену или укажите конец.",
                    Tool::Room if self.start.is_some() => "Укажите вторую точку.",
                    Tool::Room => "Укажите два противоположных угла комнаты.",
                    Tool::Wall => "Укажите начало стены или потяните от первой точки.",
                    Tool::Door | Tool::Window | Tool::Garage => {
                        "Перетащите инструмент на стену или коснитесь стены."
                    }
                    Tool::Edit => "Коснитесь стены, пола или мебели. Мебель можно перетянуть.",
                    Tool::Erase => "Щёлкните по элементу, который хотите удалить.",
                    _ => "Щёлкните по листу, чтобы поставить мебель.",
                };
                ui.label(if self.message.is_empty() {
                    hint
                } else {
                    &self.message
                });
            });
            egui::CentralPanel::default().show(ctx, |ui| self.sheet(ui, mobile, atlas));
            self.shape_menu(ctx);
            self.wall_editor(ctx);
            self.room_editor(ctx);
        } else {
            egui::TopBottomPanel::bottom("camera").show(ctx, |ui| {
                if mobile {
                    ui.spacing_mut().interact_size.y = 38.0;
                }
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

    fn tools(&mut self, ui: &mut egui::Ui, mobile: bool) {
        let tools = [
            Tool::Room,
            Tool::Wall,
            Tool::Door,
            Tool::Window,
            Tool::Garage,
            Tool::Edit,
            Tool::Bed,
            Tool::Table,
            Tool::Sofa,
            Tool::Cabinet,
            Tool::Lamp,
            Tool::Erase,
        ];
        let mut tool_buttons = |ui: &mut egui::Ui| {
            for tool in tools {
                let opening = tool.opening();
                let mut button = egui::Button::new(tool.name()).selected(self.tool == tool);
                if opening.is_some() {
                    button = button.sense(Sense::click_and_drag());
                }
                let response = ui.add(button);
                if let Some(opening) = opening {
                    response.dnd_set_drag_payload(opening);
                }
                if response.clicked() || response.drag_started() {
                    self.flush_tap();
                    self.cancel_mark();
                    self.clear_selection();
                    self.tool = tool;
                    self.message.clear();
                }
            }
        };
        if mobile {
            ui.spacing_mut().interact_size.y = 34.0;
            ui.horizontal_wrapped(&mut tool_buttons);
        } else {
            ui.heading("Инструменты");
            tool_buttons(ui);
        }
        if self.tool == Tool::Edit && self.furniture_selection.is_some() {
            self.furniture_actions(ui);
        } else {
            ui.checkbox(&mut self.rotated, "Повернуть мебель");
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.start.is_some() || self.delayed_tap.is_some() || !self.undo.is_empty(),
                    egui::Button::new("Отменить"),
                )
                .clicked()
            {
                if self.start.is_none() && self.delayed_tap.is_none() {
                    if let Some(plan) = self.undo.pop() {
                        self.plan = plan;
                    }
                }
                self.cancel_mark();
                self.clear_selection();
                self.message.clear();
            }
            if ui
                .button(if mobile {
                    "Пример"
                } else {
                    "Загрузить пример"
                })
                .clicked()
            {
                self.cancel_mark();
                self.clear_selection();
                self.load_example();
            }
            if ui.button("Формы").clicked() {
                self.cancel_mark();
                self.clear_selection();
                self.shape_at = Some(SHEET * 0.5);
            }
        });
        if !mobile {
            ui.label("Файл проекта");
            ui.text_edit_singleline(&mut self.path);
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Сохранить").clicked() {
                self.flush_tap();
                self.message = self
                    .save()
                    .map(|_| "Проект сохранён".into())
                    .unwrap_or_else(|e| e.to_string());
            }
            if ui.button("Открыть").clicked() {
                self.cancel_mark();
                self.clear_selection();
                self.message = self
                    .open()
                    .map(|_| "Проект открыт".into())
                    .unwrap_or_else(|e| e.to_string());
            }
        });
    }

    fn wall_editor(&mut self, ctx: &egui::Context) {
        let Some(edit) = &mut self.wall_edit else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Редактирование стены")
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Длина");
                    if ui.button("−").clicked() {
                        edit.length = (edit.length - 0.25).max(0.24);
                    }
                    ui.add(
                        egui::DragValue::new(&mut edit.length)
                            .speed(0.10)
                            .range(0.24..=SHEET.length())
                            .suffix(" м"),
                    );
                    if ui.button("+").clicked() {
                        edit.length = (edit.length + 0.25).min(SHEET.length());
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Толщина");
                    if ui.button("−").clicked() {
                        edit.thickness = (edit.thickness - 0.02).max(0.08);
                    }
                    ui.add(
                        egui::DragValue::new(&mut edit.thickness)
                            .speed(0.01)
                            .range(0.08..=0.80)
                            .suffix(" м"),
                    );
                    if ui.button("+").clicked() {
                        edit.thickness = (edit.thickness + 0.02).min(0.80);
                    }
                });
                apply = ui.button("Применить").clicked();
            });
        if apply {
            let previous = self.plan.clone();
            if self
                .plan
                .resize_wall(edit.index, edit.length, edit.thickness)
            {
                self.remember(previous);
                self.clear_selection();
                self.message.clear();
            } else {
                self.message = "Стена выйдет за лист или перекроет проём".into();
            }
        } else if !open {
            self.clear_selection();
        }
    }

    fn sheet(&mut self, ui: &mut egui::Ui, mobile: bool, atlas: &FurnishingAtlas) {
        let space = ui
            .available_rect_before_wrap()
            .shrink(if mobile { 4.0 } else { 22.0 });
        let portrait = self.portrait_sheet.unwrap_or(mobile);
        let dimensions = if portrait {
            Vec2::new(SHEET.y, SHEET.x)
        } else {
            SHEET
        };
        let scale = (space.width() / dimensions.x)
            .min(space.height() / dimensions.y)
            .max(1.0);
        let rect = Rect::from_center_size(
            space.center(),
            vec2(dimensions.x * scale, dimensions.y * scale),
        );
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter().with_clip_rect(rect);
        let screen = |p: Vec2| {
            let p = if portrait {
                Vec2::new(SHEET.y - p.y, p.x)
            } else {
                p
            };
            rect.min + vec2(p.x * scale, p.y * scale)
        };
        let position = |p: egui::Pos2| {
            let p = Vec2::new((p.x - rect.left()) / scale, (p.y - rect.top()) / scale);
            let p = if portrait {
                Vec2::new(p.y, SHEET.y - p.x)
            } else {
                p
            };
            p.clamp(Vec2::ZERO, SHEET)
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
        for (index, room) in self.plan.rooms.iter().enumerate() {
            let a = Vec2::from_array(room.min);
            let b = Vec2::from_array(room.max);
            let room_rect = Rect::from_two_pos(screen(a), screen(b));
            painter.rect_filled(
                room_rect,
                0.0,
                floor_color(room.finish).gamma_multiply(0.43),
            );
            if self.room_edit == Some(index) {
                painter.rect_stroke(room_rect, 0.0, Stroke::new(2.0_f32, ACCENT));
            }
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
        for (index, wall) in self.plan.walls.iter().enumerate() {
            let wall_stroke = (wall.thickness * scale).max(2.0);
            let (a, b) = wall.points();
            let selected = self
                .wall_edit
                .as_ref()
                .is_some_and(|edit| edit.index == index);
            painter.line_segment(
                [screen(a), screen(b)],
                Stroke::new(wall_stroke, if selected { ACCENT } else { INK }),
            );
            let dir = (b - a).normalize();
            for opening in &wall.openings {
                let start = a + dir * (opening.at - opening.kind.width() / 2.0);
                let end = start + dir * opening.kind.width();
                let span = [screen(start), screen(end)];
                painter.line_segment(span, Stroke::new(wall_stroke + 1.0, PAPER));
                match opening.kind {
                    OpeningKind::Window => {
                        painter.line_segment(
                            span,
                            Stroke::new(2.0_f32, Color32::from_rgb(31, 127, 174)),
                        );
                    }
                    OpeningKind::Garage => {
                        painter.line_segment(
                            span,
                            Stroke::new(2.0_f32, Color32::from_rgb(162, 95, 39)),
                        );
                    }
                    OpeningKind::Door => {
                        let open = screen(start + Vec2::new(-dir.y, dir.x) * opening.kind.width());
                        painter.line_segment([screen(start), open], Stroke::new(1.2_f32, ACCENT));
                    }
                }
            }
        }
        for (index, &stored) in self.plan.furniture.iter().enumerate() {
            let drag = self
                .furniture_drag
                .as_ref()
                .filter(|item| item.index == index);
            let item = drag.map_or(stored, |drag| drag.preview);
            let dim = item.kind.dimensions();
            let (x, z) = (dim.x * 0.5, dim.z * 0.5);
            let corners = [
                Vec2::new(-x, -z),
                Vec2::new(x, -z),
                Vec2::new(x, z),
                Vec2::new(-x, z),
            ];
            let corners = corners.map(|corner| screen(item.point(corner)));
            atlas.paint(&painter, item.kind, corners);
            if self.furniture_selection == Some(index) || drag.is_some() {
                painter.add(egui::Shape::closed_line(
                    corners.to_vec(),
                    Stroke::new(2.0_f32, ACCENT),
                ));
            }
        }
        if let Some(kind) = egui::DragAndDrop::payload::<OpeningKind>(ui.ctx()) {
            let (point, released) = ui.input(|input| {
                (
                    input
                        .pointer
                        .interact_pos()
                        .filter(|point| rect.contains(*point)),
                    input.pointer.button_released(egui::PointerButton::Primary),
                )
            });
            let target = point.and_then(|point| {
                self.plan
                    .snap_opening(*kind, position(point), (22.0 / scale).clamp(0.40, 1.3))
            });
            if let Some(target) = target {
                painter.line_segment(target.span.map(screen), Stroke::new(5.0_f32, ACCENT));
                if released {
                    let previous = self.plan.clone();
                    if self.plan.attach_opening(target) {
                        self.remember(previous);
                        self.message.clear();
                    }
                }
            } else if let Some(point) = point {
                painter.circle_stroke(
                    point,
                    7.0,
                    Stroke::new(2.0_f32, Color32::from_rgb(172, 65, 66)),
                );
                if released {
                    self.message = "Для проёма нужна подходящая свободная стена".into();
                }
            }
            if released {
                egui::DragAndDrop::take_payload::<OpeningKind>(ui.ctx());
            }
        } else if self.shape_at.is_none() && self.wall_edit.is_none() && self.room_edit.is_none() {
            let pointer = response.interact_pointer_pos().map(position);
            if response.double_clicked() {
                self.cancel_mark();
                self.shape_at = pointer.map(snap_point);
            } else if self.tool == Tool::Edit && response.drag_started() {
                self.delayed_tap = None;
                let start = ui.input(|i| i.pointer.press_origin()).map(position);
                if let Some((index, start)) =
                    start.and_then(|p| self.plan.furniture_at(p).map(|i| (i, p)))
                {
                    let item = self.plan.furniture[index];
                    let offset = Vec2::from_array(item.at) - start;
                    let preview = item_at(item, snap_point(pointer.unwrap_or(start) + offset));
                    self.furniture_drag = Some(FurnitureDrag {
                        index,
                        offset,
                        preview,
                    });
                }
            } else if self.tool == Tool::Edit && response.dragged() {
                if let (Some(drag), Some(point)) = (&mut self.furniture_drag, pointer) {
                    drag.preview = item_at(drag.preview, snap_point(point + drag.offset));
                }
            } else if self.tool == Tool::Edit && response.drag_stopped() {
                if let Some(drag) = self.furniture_drag.take() {
                    let item = pointer.map_or(drag.preview, |point| {
                        item_at(drag.preview, snap_point(point + drag.offset))
                    });
                    self.move_furniture(drag.index, item);
                    self.furniture_selection = Some(drag.index);
                }
            } else if self.tool == Tool::Wall && response.drag_started() {
                self.flush_tap();
                if self.start.is_none() {
                    self.start = ui
                        .input(|i| i.pointer.press_origin())
                        .map(position)
                        .map(snap_point);
                }
                self.wall_drag_end = pointer;
            } else if self.tool == Tool::Wall && response.dragged() {
                self.wall_drag_end = pointer.or(self.wall_drag_end);
            } else if self.tool == Tool::Wall && response.drag_stopped() {
                let end = pointer.or(self.wall_drag_end.take());
                self.wall_drag_end = None;
                if self.start.is_some() {
                    if let Some(end) = end {
                        self.apply(end);
                    }
                }
            } else if response.clicked() {
                if let Some(point) = pointer {
                    self.flush_tap();
                    self.delayed_tap = Some((point, Instant::now() + Duration::from_millis(320)));
                }
            }
        }
        if let Some((point, deadline)) = self.delayed_tap {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.flush_tap();
            } else {
                painter.circle_filled(screen(snap_point(point)), 3.0, ACCENT);
                ui.ctx().request_repaint_after(remaining);
            }
        }
        if let Some(start) = self.start {
            painter.circle_filled(screen(start), 3.0, ACCENT);
            if let Some(end) = self
                .wall_drag_end
                .or_else(|| response.hover_pos().map(position))
            {
                let end = snap_point(end);
                if self.tool == Tool::Room {
                    painter.rect_stroke(
                        Rect::from_two_pos(screen(start), screen(end)),
                        0.0,
                        Stroke::new(2.0_f32, ACCENT),
                    );
                } else {
                    painter
                        .line_segment([screen(start), screen(end)], Stroke::new(2.0_f32, ACCENT));
                    let label = format!("{:.2} м", start.distance(end));
                    painter.text(
                        screen(end) + vec2(8.0, -8.0),
                        Align2::LEFT_BOTTOM,
                        label,
                        FontId::monospace(12.0),
                        ACCENT,
                    );
                }
                painter.circle_stroke(screen(end), 3.0, Stroke::new(1.0_f32, ACCENT));
            }
        }
        painter.text(
            rect.left_bottom() + vec2(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            if portrait {
                "16 × 24 м · сетка 0,5 м"
            } else {
                "24 × 16 м · сетка 0,5 м"
            },
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
            Tool::Door | Tool::Window | Tool::Garage => self
                .tool
                .opening()
                .is_some_and(|kind| self.plan.opening(kind, point)),
            Tool::Edit => {
                self.clear_selection();
                if let Some(index) = self.plan.furniture_at(point) {
                    self.furniture_selection = Some(index);
                } else if let Some((index, _)) = self.plan.nearest_wall(point, 0.6) {
                    let wall = &self.plan.walls[index];
                    let (a, b) = wall.points();
                    self.wall_edit = Some(WallEdit {
                        index,
                        length: a.distance(b),
                        thickness: wall.thickness,
                    });
                } else {
                    self.room_edit = self.plan.room_at(point);
                }
                return;
            }
            Tool::Erase => self.plan.erase(point),
            tool => {
                let kind = match tool {
                    Tool::Bed => FurnitureKind::Bed,
                    Tool::Table => FurnitureKind::Table,
                    Tool::Cabinet => FurnitureKind::Cabinet,
                    Tool::Lamp => FurnitureKind::Lamp,
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
        let (min, max) = self.plan.preview_bounds();
        let center = (min + max) * 0.5;
        let target = Vec3::new(center.x, 40.8, center.y);
        let framing = ((max - min).length() / SHEET.length()).clamp(0.27, 1.0);
        let offset = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        ) * (self.distance * framing / aspect.clamp(0.35, 1.0));
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

fn floor_color(finish: FloorFinish) -> Color32 {
    let [r, g, b] = finish.color().map(|channel| (channel * 255.0) as u8);
    Color32::from_rgb(r, g, b)
}

fn item_at(item: Furniture, center: Vec2) -> Furniture {
    let size = item.size();
    let half = Vec2::new(size.x, size.z) * 0.5;
    Furniture {
        at: center.clamp(half, SHEET - half).to_array(),
        ..item
    }
}

fn snap_point(point: Vec2) -> Vec2 {
    (point * 2.0).round() / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_move_clamps_to_sheet_and_has_one_undo_step() {
        let mut editor = Editor {
            tool: Tool::Edit,
            ..Default::default()
        };
        let item = Furniture {
            at: [4.0, 4.0],
            kind: FurnitureKind::Bed,
            rotated: false,
        };
        editor.plan.furniture.push(item);
        editor.apply(Vec2::new(4.0, 4.0));
        assert_eq!(editor.furniture_selection, Some(0));
        let moved = item_at(
            Furniture {
                rotated: true,
                ..item
            },
            Vec2::ZERO,
        );
        editor.move_furniture(0, moved);
        assert_eq!(editor.plan.furniture[0].bounds().0, Vec2::ZERO);
        assert_eq!(editor.undo.len(), 1);
        assert_eq!(editor.undo[0].furniture[0].at, item.at);
    }

    #[test]
    fn opening_survives_touch_pointer_gone_on_drop() {
        let ctx = egui::Context::default();
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&ctx, &atlas);
        let mut editor = Editor::default();
        assert!(editor.plan.wall(Vec2::new(4.0, 8.0), Vec2::new(20.0, 8.0)));

        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 700.0));
        let frame = |editor: &mut Editor, events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ctx| editor.ui(ctx, &atlas),
            )
        };
        let layout = frame(&mut editor, vec![]);
        let source = layout
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Ворота" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("garage tool is visible");
        let destination = layout
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == PAPER => Some(rect.rect.center()),
                _ => None,
            })
            .expect("planning sheet is visible");
        let touch_button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![
                egui::Event::PointerMoved(source),
                touch_button(source, true),
            ],
        );
        frame(&mut editor, vec![egui::Event::PointerMoved(destination)]);
        assert!(egui::DragAndDrop::payload::<OpeningKind>(&ctx).is_some());
        frame(
            &mut editor,
            vec![touch_button(destination, false), egui::Event::PointerGone],
        );
        assert_eq!(editor.plan.walls[0].openings.len(), 1);
        assert!(editor.plan.walls[0].openings[0].kind == OpeningKind::Garage);
    }
}
