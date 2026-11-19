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
use crate::navigation::{self, Destination};
use crate::plan::{FloorFinish, Furniture, FurnitureKind, OpeningKind, Plan, SHEET};
use crate::scan::Photo;
use crate::sheet_edit::{HouseSelection, pencil_vertices};

use crate::theme::{self, ACCENT, INK, SHEET as PAPER};

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Room,
    Wall,
    Pencil,
    Pan,
    House,
    Door,
    Window,
    Garage,
    Edit,
    Bed,
    Table,
    Sofa,
    Cabinet,
    Lamp,
    Stair,
    StairL,
    StairU,
    Erase,
}

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Self::Room => "Комната",
            Self::Wall => "Стена",
            Self::Pencil => "Карандаш",
            Self::Pan => "Холст",
            Self::House => "Дом",
            Self::Door => "Дверь",
            Self::Window => "Окно",
            Self::Garage => "Ворота",
            Self::Edit => "Правка",
            Self::Bed => "Кровать",
            Self::Table => "Стол",
            Self::Sofa => "Диван",
            Self::Cabinet => "Шкаф",
            Self::Lamp => "Торшер",
            Self::Stair => "Лестница",
            Self::StairL => "Лестница 90°",
            Self::StairU => "Лестница 180°",
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
    pub open_projects: Option<Box<dyn Fn() -> Result<(), String>>>,
    pub close_project: Option<Box<dyn Fn() -> Result<(), String>>>,
    pub open_new_project: Option<Box<dyn Fn() -> Result<(), String>>>,
    pub project_title: String,
    pub open_photo: Option<Box<dyn Fn() -> Result<(), String>>>,
    photo_path: Option<PathBuf>,
    photo: Option<Photo>,
    photo_visible: bool,
    pub photo_wait: Option<Instant>,
    pub plan: Plan,
    pub preview: bool,
    pub modeling_open: bool,
    pub modeling: Modeling,
    catalog_open: bool,
    pub cutaway: bool,
    tool: Tool,
    start: Option<Vec2>,
    last_draw_tap: Option<(egui::Pos2, Instant)>,
    wall_drag_end: Option<Vec2>,
    shape_at: Option<Vec2>,
    wall_edit: Option<WallEdit>,
    room_edit: Option<usize>,
    furniture_selection: Option<usize>,
    furniture_drag: Option<FurnitureDrag>,
    portrait_sheet: Option<bool>,
    sheet_zoom: f32,
    sheet_pan: egui::Vec2,
    sheet_multitouch: bool,
    house_selection: Option<HouseSelection>,
    house_move: Option<(Vec2, Vec2)>,
    pencil: Vec<Vec2>,
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
            open_projects: None,
            close_project: None,
            open_new_project: None,
            project_title: "План дома".into(),
            open_photo: None,
            photo_path: None,
            photo: None,
            photo_visible: true,
            photo_wait: None,
            preview: false,
            modeling_open: false,
            modeling: Modeling::default(),
            catalog_open: false,
            cutaway: false,
            tool: Tool::Room,
            start: None,
            last_draw_tap: None,
            wall_drag_end: None,
            shape_at: None,
            wall_edit: None,
            room_edit: None,
            furniture_selection: None,
            furniture_drag: None,
            portrait_sheet: None,
            sheet_zoom: 1.0,
            sheet_pan: egui::Vec2::ZERO,
            sheet_multitouch: false,
            house_selection: None,
            house_move: None,
            pencil: Vec::new(),
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
    #[cfg(any(target_os = "android", test))]
    pub fn for_android(path: PathBuf) -> Self {
        let mut editor = Self {
            path: path.to_string_lossy().into_owned(),
            ..Self::default()
        };
        editor.modeling.set_path(path.with_file_name("model.json"));
        editor.photo_path = Some(path.with_file_name("plan-photo.gray"));
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

    fn cancel_mark(&mut self) {
        self.start = None;
        self.last_draw_tap = None;
        self.wall_drag_end = None;
        self.pencil.clear();
        self.house_move = None;
    }

    fn clear_selection(&mut self) {
        self.wall_edit = None;
        self.room_edit = None;
        self.furniture_selection = None;
        self.furniture_drag = None;
        self.house_selection = None;
        self.house_move = None;
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

    fn refresh_photo(&mut self, ctx: &egui::Context) {
        let Some(path) = &self.photo_path else { return };
        let Ok(modified) = fs::metadata(path).and_then(|meta| meta.modified()) else {
            return;
        };
        if self
            .photo
            .as_ref()
            .is_some_and(|photo| photo.modified == modified)
        {
            return;
        }
        match Photo::read(path, modified, ctx) {
            Ok(photo) => {
                self.photo = Some(photo);
                self.photo_visible = true;
                self.photo_wait = None;
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn scan_tools(&mut self, ui: &mut egui::Ui) {
        if self.open_photo.is_none() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Фото плана").clicked() {
                if let Some(open) = &self.open_photo {
                    match open() {
                        Ok(()) => self.photo_wait = Some(Instant::now() + Duration::from_secs(120)),
                        Err(error) => self.message = format!("Не удалось открыть фото: {error}"),
                    }
                }
            }
            if self.photo.is_some() {
                ui.checkbox(&mut self.photo_visible, "Подложка");
                let walls = ui.button("Распознать стены").clicked();
                let preview = ui.button("Фото в 3D").clicked();
                if (walls || preview)
                    && let Some(photo) = &self.photo
                {
                    let previous = self.plan.clone();
                    let count = photo.insert_walls(&mut self.plan, true);
                    if count > 0 {
                        self.remember(previous);
                        self.message =
                            format!("Добавлено стен: {count}. Проверьте размеры и линии.");
                    } else {
                        self.message =
                            "Новых прямых стен не найдено. Можно обвести фото вручную.".into();
                    }
                    self.preview = preview && !self.plan.walls.is_empty();
                }
            }
        });
    }

    pub fn persist_project(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.modeling_open
            || PathBuf::from(&self.path).is_file()
            || !self.plan.walls.is_empty()
            || !self.plan.rooms.is_empty()
            || !self.plan.furniture.is_empty()
        {
            self.save()?;
        }
        if self.modeling_open || self.modeling.has_saved_model() {
            self.modeling.save()?;
        }
        Ok(())
    }

    fn menu_action(&mut self, project_action: ProjectAction) {
        if let Err(error) = self.persist_project() {
            self.message = format!("Не удалось сохранить проект: {error}");
            return;
        }
        let action = match project_action {
            ProjectAction::Open => &self.open_projects,
            ProjectAction::Close => &self.close_project,
            ProjectAction::New => &self.open_new_project,
            ProjectAction::Updates => &self.open_updates,
        };
        if let Some(open) = action
            && let Err(error) = open()
        {
            self.message = format!("Не удалось открыть экран: {error}");
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
            self.sheet_multitouch = false;
        }
        self.refresh_photo(ctx);
        let mobile = cfg!(target_os = "android");
        egui::TopBottomPanel::top("toolbar")
            .frame(theme::toolbar(ctx))
            .show(ctx, |ui| {
                ui.spacing_mut().interact_size.y = if mobile { 30.0 } else { 26.0 };
                ui.horizontal(|ui| {
                    brand(ui);
                    ui.label(
                        RichText::new("WHP3")
                            .strong()
                            .color(ui.visuals().hyperlink_color),
                    );
                    let has_projects = self.open_projects.is_some();
                    let tab_width =
                        (ui.available_width() - if has_projects { 104.0 } else { 42.0 }).max(28.0);
                    let tab = egui::Frame::none()
                        .fill(ui.visuals().selection.bg_fill)
                        .rounding(9.0_f32)
                        .inner_margin(egui::Margin::symmetric(8.0, 5.0));
                    let current_tab = tab.show(ui, |ui| {
                        ui.set_width((tab_width - 16.0).max(12.0));
                        ui.add(
                            egui::Label::new(RichText::new(&self.project_title).strong())
                                .truncate(),
                        );
                    });
                    if has_projects
                        && current_tab
                            .response
                            .interact(Sense::click())
                            .on_hover_text("Мои проекты")
                            .clicked()
                    {
                        self.menu_action(ProjectAction::Open);
                    }
                    if has_projects && ui.button("+").on_hover_text("Новый проект").clicked()
                    {
                        self.menu_action(ProjectAction::New);
                    }
                    if has_projects && ui.button("×").on_hover_text("Закрыть проект").clicked()
                    {
                        self.menu_action(ProjectAction::Close);
                    }
                    ui.menu_button("...", |ui| {
                        ui.set_max_width(240.0);
                        if has_projects && ui.button("Мои проекты").clicked() {
                            self.menu_action(ProjectAction::Open);
                            ui.close_menu();
                        }
                        if self.open_updates.is_some() && ui.button("Обновления").clicked()
                        {
                            self.menu_action(ProjectAction::Updates);
                            ui.close_menu();
                        }
                        if !self.preview && !self.modeling_open {
                            let current = self.portrait_sheet.unwrap_or(mobile);
                            ui.horizontal_wrapped(|ui| {
                                let portrait = ui.selectable_label(current, "Портрет").clicked();
                                let landscape = ui.selectable_label(!current, "Альбом").clicked();
                                if portrait || landscape {
                                    self.cancel_mark();
                                    if portrait != current {
                                        self.sheet_zoom = 1.0;
                                        self.sheet_pan = egui::Vec2::ZERO;
                                    }
                                    self.portrait_sheet = Some(portrait);
                                    ui.close_menu();
                                }
                            });
                            self.scan_tools(ui);
                        }
                    });
                });
            });
        let active = if self.modeling_open {
            Destination::Scene
        } else if self.preview {
            Destination::House
        } else if self.catalog_open {
            Destination::Catalog
        } else {
            Destination::Sheet
        };
        egui::TopBottomPanel::bottom("navigation")
            .frame(theme::toolbar(ctx))
            .show(ctx, |ui| {
                if let Some(destination) = navigation::bar(ui, active) {
                    self.select_destination(destination);
                }
                if mobile {
                    ui.add_space(16.0);
                }
            });
        if self.modeling_open {
            self.modeling.sync_house(&self.plan);
            self.modeling.ui(ctx, mobile);
            return;
        }
        if !self.preview {
            if self.catalog_open {
                if mobile {
                    egui::TopBottomPanel::bottom("catalog")
                        .frame(theme::toolbar(ctx))
                        .show(ctx, |ui| {
                            let max = (ctx.screen_rect().height() * 0.46).clamp(155.0, 305.0);
                            egui::ScrollArea::vertical().max_height(max).show(ui, |ui| {
                                self.tool_hint(ui);
                                self.tools(ui, true);
                            });
                        });
                } else {
                    egui::SidePanel::left("catalog")
                        .exact_width(180.0)
                        .frame(theme::toolbar(ctx))
                        .show(ctx, |ui| {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                self.tool_hint(ui);
                                self.tools(ui, false);
                            });
                        });
                }
            }
            egui::CentralPanel::default().show(ctx, |ui| self.sheet(ui, mobile, atlas));
            self.shape_menu(ctx);
            self.wall_editor(ctx);
            self.room_editor(ctx);
        } else {
            egui::TopBottomPanel::bottom("camera")
                .frame(theme::toolbar(ctx))
                .show(ctx, |ui| {
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

    fn select_destination(&mut self, destination: Destination) {
        self.cancel_mark();
        self.shape_at = None;
        self.clear_selection();
        self.message.clear();
        self.preview = destination == Destination::House;
        self.modeling_open = destination == Destination::Scene;
        self.catalog_open = destination == Destination::Catalog;
    }

    fn tool_hint(&self, ui: &mut egui::Ui) {
        let hint = match self.tool {
            Tool::Wall if self.start.is_some() => "Потяните стену или укажите конец.",
            Tool::Room if self.start.is_some() => "Укажите вторую точку.",
            Tool::Room => "Укажите два противоположных угла комнаты.",
            Tool::Wall => "Укажите начало стены или потяните от первой точки.",
            Tool::Pencil => "Проведите пальцем, чтобы начертить цепочку стен.",
            Tool::Pan => {
                "Ведите пальцем для сдвига. Сведите или разведите два пальца для масштаба."
            }
            Tool::House => "Коснитесь дома и перетащите его в новое место.",
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
    }

    fn tools(&mut self, ui: &mut egui::Ui, mobile: bool) {
        let tools = [
            Tool::Room,
            Tool::Wall,
            Tool::Pencil,
            Tool::House,
            Tool::Pan,
            Tool::Door,
            Tool::Window,
            Tool::Garage,
            Tool::Edit,
            Tool::Bed,
            Tool::Table,
            Tool::Sofa,
            Tool::Cabinet,
            Tool::Lamp,
            Tool::Stair,
            Tool::StairL,
            Tool::StairU,
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
        if self.tool == Tool::Pan {
            ui.horizontal_wrapped(|ui| {
                if ui.button("−").clicked() {
                    self.sheet_zoom = (self.sheet_zoom / 1.25).max(0.75);
                }
                if ui.button("+").clicked() {
                    self.sheet_zoom = (self.sheet_zoom * 1.25).min(5.0);
                }
                if ui.button("Вписать лист").clicked() {
                    self.sheet_pan = egui::Vec2::ZERO;
                    self.sheet_zoom = 1.0;
                }
            });
        } else if self.tool == Tool::Edit && self.furniture_selection.is_some() {
            self.furniture_actions(ui);
        } else {
            ui.checkbox(&mut self.rotated, "Повернуть мебель");
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.start.is_some() || !self.undo.is_empty(),
                    egui::Button::new("Отменить"),
                )
                .clicked()
            {
                if self.start.is_none() {
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
        let (viewport, response) =
            ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let space = viewport.shrink(if mobile { 4.0 } else { 22.0 });
        let portrait = self.portrait_sheet.unwrap_or(mobile);
        let dimensions = if portrait {
            Vec2::new(SHEET.y, SHEET.x)
        } else {
            SHEET
        };
        let base_scale = (space.width() / dimensions.x)
            .min(space.height() / dimensions.y)
            .max(1.0);
        let (pinch, down, scroll) = ui.input(|input| {
            (
                input
                    .multi_touch()
                    .filter(|touch| viewport.contains(touch.start_pos)),
                input.pointer.any_down(),
                input.smooth_scroll_delta.y,
            )
        });
        if let Some(pinch) = pinch {
            self.sheet_multitouch = true;
            self.cancel_mark();
            self.furniture_drag = None;
            egui::DragAndDrop::clear_payload(ui.ctx());
            let zoom = (self.sheet_zoom * pinch.zoom_delta).clamp(0.75, 5.0);
            let anchor = pinch.start_pos - space.center() - self.sheet_pan;
            self.sheet_pan += pinch.translation_delta + anchor * (1.0 - zoom / self.sheet_zoom);
            self.sheet_zoom = zoom;
        } else if !self.sheet_multitouch {
            if self.tool == Tool::Pan && response.dragged() {
                self.sheet_pan += response.drag_delta();
            }
            if response.hovered() && scroll != 0.0 {
                let zoom = (self.sheet_zoom * (scroll * 0.002).exp()).clamp(0.75, 5.0);
                if let Some(point) = response.hover_pos() {
                    self.sheet_pan +=
                        (point - space.center() - self.sheet_pan) * (1.0 - zoom / self.sheet_zoom);
                }
                self.sheet_zoom = zoom;
            }
        }
        let block_editing = self.sheet_multitouch || self.tool == Tool::Pan;
        if !down && pinch.is_none() {
            self.sheet_multitouch = false;
        }
        let scale = base_scale * self.sheet_zoom;
        let size = vec2(dimensions.x * scale, dimensions.y * scale);
        let pan_limit = size * 0.5 + viewport.size() * 0.40;
        self.sheet_pan = self.sheet_pan.clamp(-pan_limit, pan_limit);
        let rect = Rect::from_center_size(space.center() + self.sheet_pan, size);
        let painter = ui.painter().with_clip_rect(viewport.intersect(rect));
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
        painter.rect_stroke(rect, 0.0, Stroke::new(1.0_f32, theme::BORDER));
        if self.photo_visible
            && let Some(photo) = &self.photo
        {
            let corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
            let mut mesh = egui::Mesh::with_texture(photo.texture.id());
            for [u, v] in corners {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: screen(photo.mapped_point([u, v], true)),
                    uv: egui::pos2(u, v),
                    color: Color32::WHITE,
                });
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
        for x in 0..=(SHEET.x * 2.0) as usize {
            let x_m = x as f32 * 0.5;
            let stroke: f32 = if x % 2 == 0 { 0.8 } else { 0.4 };
            painter.line_segment(
                [screen(Vec2::new(x_m, 0.0)), screen(Vec2::new(x_m, SHEET.y))],
                Stroke::new(stroke, theme::BORDER),
            );
        }
        for y in 0..=(SHEET.y * 2.0) as usize {
            let y_m = y as f32 * 0.5;
            let stroke: f32 = if y % 2 == 0 { 0.8 } else { 0.4 };
            painter.line_segment(
                [screen(Vec2::new(0.0, y_m)), screen(Vec2::new(SHEET.x, y_m))],
                Stroke::new(stroke, theme::BORDER),
            );
        }
        let house_delta = self.house_move.map_or(Vec2::ZERO, |(_, delta)| delta);
        for (index, room) in self.plan.rooms.iter().enumerate() {
            let selected = self
                .house_selection
                .as_ref()
                .is_some_and(|house| house.rooms.get(index) == Some(&true));
            let shift = if selected { house_delta } else { Vec2::ZERO };
            let a = Vec2::from_array(room.min) + shift;
            let b = Vec2::from_array(room.max) + shift;
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
            let in_house = self
                .house_selection
                .as_ref()
                .is_some_and(|house| house.walls.get(index) == Some(&true));
            let shift = if in_house { house_delta } else { Vec2::ZERO };
            let (a, b) = (a + shift, b + shift);
            let selected = in_house
                || self
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
            let in_house = self
                .house_selection
                .as_ref()
                .is_some_and(|house| house.furniture.get(index) == Some(&true));
            let shift = if in_house { house_delta } else { Vec2::ZERO };
            let corners = corners.map(|corner| screen(item.point(corner) + shift));
            atlas.paint(&painter, item.kind, corners);
            if self.furniture_selection == Some(index) || drag.is_some() {
                painter.add(egui::Shape::closed_line(
                    corners.to_vec(),
                    Stroke::new(2.0_f32, ACCENT),
                ));
            }
        }
        if let Some(house) = &self.house_selection {
            let selection = Rect::from_two_pos(
                screen(house.min + house_delta),
                screen(house.max + house_delta),
            );
            painter.rect_stroke(selection.expand(4.0), 4.0, Stroke::new(1.5_f32, ACCENT));
        }
        if self.pencil.len() > 1 {
            painter.add(egui::Shape::line(
                self.pencil.iter().copied().map(screen).collect(),
                Stroke::new(2.2_f32, ACCENT),
            ));
        }
        if block_editing {
            // A multi-touch or pan gesture must not commit any edit underneath it.
        } else if let Some(kind) = egui::DragAndDrop::payload::<OpeningKind>(ui.ctx()) {
            let (point, released) = ui.input(|input| {
                (
                    input
                        .pointer
                        .interact_pos()
                        .filter(|point| viewport.intersect(rect).contains(*point)),
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
            let on_sheet = response
                .interact_pointer_pos()
                .is_some_and(|point| rect.contains(point));
            if self.tool == Tool::House {
                if response.clicked() {
                    self.house_selection = if on_sheet {
                        pointer.and_then(|point| HouseSelection::at(&self.plan, point))
                    } else {
                        None
                    };
                } else if response.drag_started() {
                    let start = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|point| rect.contains(*point))
                        .map(position);
                    if let Some(start) = start {
                        self.house_selection = HouseSelection::at(&self.plan, start);
                        if let (Some(house), Some(end)) = (&self.house_selection, pointer) {
                            self.house_move =
                                Some((start, house.clamp_delta(snap_point(end - start))));
                        }
                    }
                } else if response.dragged() {
                    if let (Some((start, _)), Some(house), Some(end)) =
                        (self.house_move, &self.house_selection, pointer)
                    {
                        self.house_move = Some((start, house.clamp_delta(snap_point(end - start))));
                        ui.ctx().request_repaint();
                    }
                } else if response.drag_stopped() {
                    if let (Some((start, delta)), Some(house)) =
                        (self.house_move.take(), &mut self.house_selection)
                    {
                        let delta =
                            pointer.map_or(delta, |end| house.clamp_delta(snap_point(end - start)));
                        if delta != Vec2::ZERO {
                            let previous = self.plan.clone();
                            house.translate(&mut self.plan, delta);
                            self.remember(previous);
                            self.message.clear();
                        }
                    }
                }
            } else if self.tool == Tool::Pencil {
                if response.drag_started() {
                    self.pencil.clear();
                    if let Some(start) = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|point| rect.contains(*point))
                        .map(position)
                    {
                        self.pencil.push(start);
                    }
                }
                if response.dragged()
                    && let Some(point) = pointer
                    && self
                        .pencil
                        .last()
                        .is_some_and(|last| last.distance(point) >= 0.08)
                {
                    if self.pencil.len() < 2048 {
                        self.pencil.push(point);
                    }
                    ui.ctx().request_repaint();
                }
                if response.drag_stopped() && !self.pencil.is_empty() {
                    if let Some(point) = pointer {
                        self.pencil.push(point);
                    }
                    let points = pencil_vertices(&self.pencil);
                    self.pencil.clear();
                    if points.len() > 1 {
                        let previous = self.plan.clone();
                        let mut added = false;
                        for pair in points.windows(2) {
                            if self.plan.walls.len() >= 2000 {
                                break;
                            }
                            added |= self.plan.wall(pair[0], pair[1]);
                        }
                        if added {
                            self.remember(previous);
                            self.message.clear();
                        }
                    }
                }
            } else if self.tool == Tool::Edit && response.drag_started() {
                self.last_draw_tap = None;
                let start = ui
                    .input(|i| i.pointer.press_origin())
                    .filter(|point| rect.contains(*point))
                    .map(position);
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
                if self.start.is_none() {
                    self.start = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|point| rect.contains(*point))
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
            } else if response.clicked()
                && on_sheet
                && let Some(pointer) = response.interact_pointer_pos()
            {
                self.tap_sheet(pointer, position(pointer));
                ui.ctx().request_repaint();
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

    fn tap_sheet(&mut self, pointer: egui::Pos2, point: Vec2) {
        let is_drawing = matches!(self.tool, Tool::Room | Tool::Wall);
        let now = Instant::now();
        let near_previous = self.last_draw_tap.is_some_and(|(previous, time)| {
            now.duration_since(time) <= Duration::from_millis(320)
                && previous.distance(pointer) <= 14.0
        });
        if is_drawing && self.start.is_some() && near_previous {
            self.cancel_mark();
            self.shape_at = Some(snap_point(point));
        } else {
            self.apply(point);
            self.last_draw_tap = (is_drawing && self.start.is_some()).then_some((pointer, now));
        }
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
            Tool::Pencil | Tool::Pan | Tool::House => return,
            Tool::Erase => self.plan.erase(point),
            tool => {
                let kind = match tool {
                    Tool::Bed => FurnitureKind::Bed,
                    Tool::Table => FurnitureKind::Table,
                    Tool::Cabinet => FurnitureKind::Cabinet,
                    Tool::Lamp => FurnitureKind::Lamp,
                    Tool::Stair => FurnitureKind::Stair,
                    Tool::StairL => FurnitureKind::StairL,
                    Tool::StairU => FurnitureKind::StairU,
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

enum ProjectAction {
    Open,
    Close,
    New,
    Updates,
}

fn brand(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, ACCENT);
    let origin = rect.min;
    let at = |x, y| origin + vec2(x, y);
    painter.add(egui::Shape::line(
        vec![at(5.0, 12.0), at(12.0, 6.0), at(19.0, 12.0)],
        Stroke::new(1.6_f32, Color32::WHITE),
    ));
    painter.rect_stroke(
        Rect::from_min_max(at(8.0, 12.0), at(16.0, 19.0)),
        1.0,
        Stroke::new(1.4_f32, Color32::WHITE),
    );
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
    fn project_directories_keep_plans_and_photo_paths_separate() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "homeplanner-projects-{}-{stamp}",
            std::process::id()
        ));
        let first = root.join("first");
        let second = root.join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let mut editor = Editor::for_android(first.join("house.json"));
        editor.plan.wall(Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0));
        editor.persist_project().unwrap();
        let mut editor = Editor::for_android(second.join("house.json"));
        assert!(editor.plan.walls.is_empty());
        assert_eq!(editor.photo_path, Some(second.join("plan-photo.gray")));
        editor.plan.wall(Vec2::new(3.0, 3.0), Vec2::new(8.0, 3.0));
        editor.persist_project().unwrap();

        let reopened = Editor::for_android(first.join("house.json"));
        assert_eq!(reopened.photo_path, Some(first.join("plan-photo.gray")));
        assert_eq!(reopened.plan.walls[0].from, [2.0, 2.0]);
        fs::remove_dir_all(root).unwrap();
    }

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
    fn touch_taps_draw_a_room_and_place_furniture_without_waiting_for_a_timer() {
        let ctx = egui::Context::default();
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&ctx, &atlas);
        let mut editor = Editor::default();
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 700.0));
        let frame = |editor: &mut Editor, events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| editor.ui(ctx, &atlas),
            )
        };
        let layout = frame(&mut editor, vec![]);
        let center = layout
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == PAPER => Some(rect.rect.center()),
                _ => None,
            })
            .expect("planning sheet is visible");
        let tap = |editor: &mut Editor, pos| {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                pressed,
                button: egui::PointerButton::Primary,
                modifiers: egui::Modifiers::NONE,
            };
            frame(editor, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(editor, vec![button(false), egui::Event::PointerGone]);
        };

        tap(&mut editor, center - vec2(70.0, 60.0));
        assert!(editor.start.is_some());
        tap(&mut editor, center + vec2(70.0, 60.0));
        assert_eq!(editor.plan.rooms.len(), 1);
        assert!(editor.shape_at.is_none());
        editor.tool = Tool::Table;
        tap(&mut editor, center);
        assert_eq!(editor.plan.furniture.len(), 1);
    }

    #[test]
    fn pinching_the_sheet_zooms_without_drawing_walls() {
        let ctx = egui::Context::default();
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&ctx, &atlas);
        let mut editor = Editor {
            tool: Tool::Pencil,
            ..Default::default()
        };
        let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 700.0));
        let frame = |editor: &mut Editor, events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    events,
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| editor.ui(ctx, &atlas),
            )
        };
        let layout = frame(&mut editor, vec![]);
        let center = layout
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == PAPER => Some(rect.rect.center()),
                _ => None,
            })
            .expect("planning sheet is visible");
        let a = center - vec2(20.0, 0.0);
        let b = center + vec2(20.0, 0.0);
        let touch = |id, phase, pos| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(id),
            phase,
            pos,
            force: None,
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![
                touch(1, egui::TouchPhase::Start, a),
                egui::Event::PointerMoved(a),
                button(a, true),
            ],
        );
        frame(&mut editor, vec![touch(2, egui::TouchPhase::Start, b)]);
        let moved_a = a - vec2(12.0, 0.0);
        let moved_b = b + vec2(12.0, 0.0);
        frame(
            &mut editor,
            vec![
                touch(1, egui::TouchPhase::Move, moved_a),
                touch(2, egui::TouchPhase::Move, moved_b),
                egui::Event::PointerMoved(moved_a),
            ],
        );
        frame(&mut editor, vec![touch(2, egui::TouchPhase::End, moved_b)]);
        frame(
            &mut editor,
            vec![
                touch(1, egui::TouchPhase::End, moved_a),
                button(moved_a, false),
                egui::Event::PointerGone,
            ],
        );
        assert!(editor.sheet_zoom > 1.0);
        assert!(editor.plan.walls.is_empty());
    }

    #[test]
    fn opening_survives_touch_pointer_gone_on_drop() {
        let ctx = egui::Context::default();
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&ctx, &atlas);
        let mut editor = Editor {
            catalog_open: true,
            ..Default::default()
        };
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
