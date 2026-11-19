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
use crate::plan::{
    copy_furniture, item_at, snap_point, FloorFinish, Furniture, FurnitureKind, OpeningKind, Plan,
    SHEET,
};
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

    fn furnishing(self) -> Option<FurnitureKind> {
        match self {
            Self::Bed => Some(FurnitureKind::Bed),
            Self::Table => Some(FurnitureKind::Table),
            Self::Sofa => Some(FurnitureKind::Sofa),
            Self::Cabinet => Some(FurnitureKind::Cabinet),
            Self::Lamp => Some(FurnitureKind::Lamp),
            Self::Stair => Some(FurnitureKind::Stair),
            Self::StairL => Some(FurnitureKind::StairL),
            Self::StairU => Some(FurnitureKind::StairU),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CatalogCategory {
    Furniture,
    Stairs,
    Edit,
}

impl CatalogCategory {
    const ALL: [Self; 3] = [Self::Furniture, Self::Stairs, Self::Edit];

    fn title(self) -> &'static str {
        match self {
            Self::Furniture => "Мебель",
            Self::Stairs => "Лестницы",
            Self::Edit => "Правка",
        }
    }

    fn tools(self) -> &'static [Tool] {
        match self {
            Self::Furniture => &[
                Tool::Bed,
                Tool::Table,
                Tool::Sofa,
                Tool::Cabinet,
                Tool::Lamp,
            ],
            Self::Stairs => &[Tool::Stair, Tool::StairL, Tool::StairU],
            Self::Edit => &[Tool::House, Tool::Pan, Tool::Edit, Tool::Erase],
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

#[derive(Clone, Copy)]
enum OutlineAction {
    Move,
    Wall,
    EditWall,
    Split(usize),
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
    outline_room: Option<usize>,
    outline_action: Option<OutlineAction>,
    outline_anchor: Option<egui::Pos2>,
    pencil: Vec<Vec2>,
    rotated: bool,
    undo: Vec<Plan>,
    redo: Vec<Plan>,
    plan_dirty: bool,
    catalog_category: CatalogCategory,
    build_open: bool,
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
            outline_room: None,
            outline_action: None,
            outline_anchor: None,
            pencil: Vec::new(),
            rotated: false,
            undo: vec![],
            redo: vec![],
            plan_dirty: false,
            catalog_category: CatalogCategory::Furniture,
            build_open: false,
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
            editor.plan_dirty = false;
        }
        editor
    }

    fn remember(&mut self, previous: Plan) {
        self.redo.clear();
        self.plan_dirty = true;
        self.push_undo(previous);
    }

    fn remember_wall_changes(&mut self, previous: Plan) {
        self.plan.detect_partitions();
        if let Some((a, b)) = self.outline_bounds() {
            self.house_selection = HouseSelection::at(&self.plan, (a + b) * 0.5);
        }
        self.remember(previous);
    }

    fn push_undo(&mut self, previous: Plan) {
        if self.undo.len() == 40 {
            self.undo.remove(0);
        }
        self.undo.push(previous);
    }

    fn can_undo(&self) -> bool {
        self.start.is_some() || !self.pencil.is_empty() || !self.undo.is_empty()
    }

    fn undo_plan(&mut self, all: bool) {
        if all || (self.start.is_none() && self.pencil.is_empty()) {
            while let Some(plan) = self.undo.pop() {
                self.redo.push(std::mem::replace(&mut self.plan, plan));
                self.plan_dirty = true;
                if !all {
                    break;
                }
            }
        }
        self.reset_edit_state();
    }

    fn redo_plan(&mut self) {
        if let Some(plan) = self.redo.pop() {
            let previous = std::mem::replace(&mut self.plan, plan);
            self.push_undo(previous);
            self.plan_dirty = true;
            self.reset_edit_state();
        }
    }

    fn reset_edit_state(&mut self) {
        self.cancel_mark();
        self.clear_selection();
        self.message.clear();
    }

    fn fit_sheet(&mut self) {
        self.sheet_pan = egui::Vec2::ZERO;
        self.sheet_zoom = 1.0;
    }

    fn sheet_actions(&mut self, ui: &mut egui::Ui) {
        use navigation::SheetAction;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let width = ((ui.available_width() - 39.0) / 3.0).min(105.0);
            for (action, enabled) in [
                (SheetAction::Undo, self.can_undo()),
                (SheetAction::Redo, !self.redo.is_empty()),
                (SheetAction::Fit, true),
            ] {
                if action.pill(ui, width, enabled).clicked() {
                    match action {
                        SheetAction::Undo => self.undo_plan(false),
                        SheetAction::Redo => self.redo_plan(),
                        SheetAction::Fit => self.fit_sheet(),
                    }
                }
            }
            ui.spacing_mut().interact_size.y = 38.0;
            ui.menu_button("...", |ui| {
                if ui
                    .selectable_label(self.tool == Tool::House, "Контур дома")
                    .clicked()
                {
                    self.reset_edit_state();
                    self.tool = Tool::House;
                    ui.close_menu();
                }
                if ui
                    .add_enabled(self.can_undo(), egui::Button::new("Отменить всё"))
                    .clicked()
                {
                    self.undo_plan(true);
                    ui.close_menu();
                }
            })
            .response
            .on_hover_text("Действия с холстом");
        });
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
        crate::write_project_file(&path, &serde_json::to_vec_pretty(&self.plan)?)?;
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
        self.outline_room = None;
        self.outline_action = None;
        self.outline_anchor = None;
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
                if let Some(copy) = copy_furniture(&self.plan, item) {
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

    fn outline_bounds(&self) -> Option<(Vec2, Vec2)> {
        let room = self.plan.rooms.get(self.outline_room?)?;
        Some((Vec2::from_array(room.min), Vec2::from_array(room.max)))
    }

    fn outline_tap(&mut self, point: Vec2) {
        if !point.cmpge(Vec2::ZERO).all() || !point.cmple(SHEET).all() {
            return;
        }
        match self.outline_action {
            Some(OutlineAction::Wall) => {
                let point = snap_point(point);
                if let Some(start) = self.start.take() {
                    let previous = self.plan.clone();
                    if self.plan.wall(start, point) {
                        self.remember_wall_changes(previous);
                    }
                } else {
                    self.start = Some(point);
                }
            }
            Some(OutlineAction::Split(axis)) => {
                if let Some((a, b)) = self.outline_bounds()
                    && point.cmpge(a).all()
                    && point.cmple(b).all()
                    && let Some(index) = self.outline_room
                {
                    self.split_outline(index, axis, Some(point[axis]));
                }
            }
            Some(OutlineAction::EditWall) => {
                if let Some((index, _)) = self.plan.nearest_wall(point, 0.6) {
                    self.outline_action = None;
                    self.edit_wall(index);
                }
            }
            Some(OutlineAction::Move) => {
                if let Some((a, b)) = self.outline_bounds()
                    && let Some(house) = &mut self.house_selection
                {
                    let delta = house.clamp_delta(snap_point(point - (a + b) * 0.5));
                    if delta != Vec2::ZERO {
                        let previous = self.plan.clone();
                        house.translate(&mut self.plan, delta);
                        self.remember(previous);
                    }
                }
                self.outline_action = None;
            }
            None => {
                self.outline_room = self.plan.room_at(point);
                self.house_selection = HouseSelection::at(&self.plan, point);
                if self.outline_room.is_none()
                    && let Some((index, _)) = self.plan.nearest_wall(point, 0.45)
                {
                    self.edit_wall(index);
                }
            }
        }
    }

    fn edit_wall(&mut self, index: usize) {
        let wall = &self.plan.walls[index];
        let (a, b) = wall.points();
        self.wall_edit = Some(WallEdit {
            index,
            length: a.distance(b),
            thickness: wall.thickness,
        });
    }

    fn split_outline(&mut self, index: usize, axis: usize, at: Option<f32>) {
        let previous = self.plan.clone();
        if self.plan.split_room(index, axis, at).is_some() {
            self.remember(previous);
            self.shape_at = None;
            self.tool = Tool::House;
            self.outline_room = Some(index);
            self.outline_action = None;
            if let Some((a, b)) = self.outline_bounds() {
                self.house_selection = HouseSelection::at(&self.plan, (a + b) * 0.5);
            }
            self.message.clear();
        } else {
            self.message = "Оставьте хотя бы 0,5 м с каждой стороны".into();
        }
    }

    fn outline_options(&mut self, ui: &mut egui::Ui, index: usize) {
        ui.menu_button("Разделить на комнаты", |ui| {
            for (axis, direction) in [(0, "Вдоль Y"), (1, "Вдоль X")] {
                ui.horizontal(|ui| {
                    ui.label(direction);
                    if ui.button("Пополам").clicked() {
                        self.split_outline(index, axis, None);
                        ui.close_menu();
                    }
                    if ui.button("По точке").clicked() {
                        self.shape_at = None;
                        self.tool = Tool::House;
                        self.outline_room = Some(index);
                        self.outline_action = Some(OutlineAction::Split(axis));
                        self.message.clear();
                        ui.close_menu();
                    }
                });
            }
        });
        for (action, label) in [
            (OutlineAction::Move, "Перенести коробку"),
            (OutlineAction::Wall, "Свободная стена"),
        ] {
            if ui.button(label).clicked() {
                self.shape_at = None;
                self.start = None;
                self.tool = Tool::House;
                self.outline_room = Some(index);
                if let Some((a, b)) = self.outline_bounds() {
                    self.house_selection = HouseSelection::at(&self.plan, (a + b) * 0.5);
                }
                self.outline_action = Some(action);
                self.message.clear();
            }
        }
        if ui.button("Править стену").clicked() {
            self.shape_at = None;
            self.tool = Tool::House;
            self.outline_room = Some(index);
            self.outline_action = Some(OutlineAction::EditWall);
        }
        if ui.button("Цвет пола").clicked() {
            self.shape_at = None;
            self.room_edit = Some(index);
        }
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
    }

    fn outline_menu(&mut self, ctx: &egui::Context) {
        if self.shape_at.is_some() || self.wall_edit.is_some() || self.room_edit.is_some() {
            return;
        }
        let anchor = self
            .outline_anchor
            .unwrap_or_else(|| ctx.screen_rect().center());
        let Some(index) = self.outline_room else {
            return;
        };
        let viewport = ctx.screen_rect();
        let right = (viewport.right() - 240.0).max(viewport.left() + 8.0);
        let below = (viewport.bottom() - 310.0).max(viewport.top() + 8.0);
        let position = egui::pos2(
            anchor.x.clamp(viewport.left() + 8.0, right),
            anchor.y.clamp(viewport.top() + 8.0, below),
        );
        let mut open = true;
        egui::Window::new("Контур комнаты")
            .current_pos(position)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_max_width(215.0);
                ui.spacing_mut().interact_size.y = 30.0;
                match self.outline_action {
                    None => self.outline_options(ui, index),
                    Some(action) => {
                        ui.label(match action {
                            OutlineAction::Move => "Коснитесь нового центра",
                            OutlineAction::EditWall => "Коснитесь стены",
                            OutlineAction::Split(_) => "Укажите точку перегородки",
                            OutlineAction::Wall if self.start.is_none() => "Укажите начало стены",
                            OutlineAction::Wall => "Укажите конец стены",
                        });
                        if !self.message.is_empty() {
                            ui.label(&self.message);
                        }
                        if ui.button("Готово").clicked() {
                            self.start = None;
                            self.outline_action = None;
                        }
                    }
                }
            });
        if !open {
            self.reset_edit_state();
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
                if let Some(index) = self.plan.room_at(point) {
                    self.outline_options(ui, index);
                    ui.separator();
                }
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

    #[cfg(any(target_os = "android", test))]
    pub fn autosave_project(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.plan_dirty {
            self.save()?;
            self.plan_dirty = false;
        }
        self.modeling.autosave()
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
            self.build_open = false;
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
            let outline = self.outline_bounds();
            let output = self.modeling.ui(
                ctx,
                mobile,
                outline,
                self.start,
                &mut self.plan,
                &mut self.furniture_selection,
            );
            if let Some(previous) = output.plan_changed {
                self.remember(previous);
            }
            if let Some((point, screen)) = output.plan_tap {
                if self.room_edit.is_none() && self.wall_edit.is_none() {
                    if self.outline_action.is_none() {
                        self.outline_anchor = Some(screen + vec2(14.0, 14.0));
                    }
                    self.outline_tap(point);
                }
            }
            if self.modeling.editing_plan() {
                self.outline_menu(ctx);
                self.wall_editor(ctx);
                self.room_editor(ctx);
            }
            return;
        }
        if !self.preview {
            if self.catalog_open {
                if mobile {
                    egui::TopBottomPanel::bottom("catalog")
                        .frame(theme::toolbar(ctx))
                        .show(ctx, |ui| {
                            self.catalog(ui, atlas, true);
                        });
                } else {
                    egui::SidePanel::left("catalog")
                        .exact_width(280.0)
                        .frame(theme::toolbar(ctx))
                        .show(ctx, |ui| {
                            self.catalog(ui, atlas, false);
                        });
                }
            } else {
                egui::TopBottomPanel::bottom("sheet_actions")
                    .frame(theme::toolbar(ctx))
                    .show(ctx, |ui| self.sheet_actions(ui));
            }
            let canvas = ctx.available_rect();
            egui::CentralPanel::default().show(ctx, |ui| self.sheet(ui, mobile, atlas));
            if !self.catalog_open {
                self.build_menu(ctx, canvas);
            }
            self.shape_menu(ctx);
            self.outline_menu(ctx);
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
        self.build_open = false;
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

    fn build_menu(&mut self, ctx: &egui::Context, canvas: Rect) {
        if self.shape_at.is_some()
            || self.outline_room.is_some()
            || self.wall_edit.is_some()
            || self.room_edit.is_some()
        {
            return;
        }
        use navigation::BuildIcon;
        let center = egui::pos2(canvas.center().x, canvas.bottom() - 31.0);
        let (label, icon) = if self.build_open {
            ("Закрыть", BuildIcon::Close)
        } else {
            ("Стройка", BuildIcon::Room)
        };
        if icon
            .floating(
                ctx,
                "build_toggle",
                center,
                label,
                self.build_open,
                Sense::click(),
            )
            .clicked()
        {
            self.build_open = !self.build_open;
        }
        if !self.build_open {
            return;
        }

        let actions = [
            (Some(Tool::Room), BuildIcon::Room),
            (Some(Tool::Wall), BuildIcon::Wall),
            (Some(Tool::Pencil), BuildIcon::Pencil),
            (Some(Tool::Door), BuildIcon::Door),
            (Some(Tool::Window), BuildIcon::Window),
            (Some(Tool::Garage), BuildIcon::Garage),
            (None, BuildIcon::Shapes),
        ];
        let radius = ((canvas.width() - 65.0) * 0.5).clamp(65.0, 122.0);
        let rise = (canvas.height() - 62.0).clamp(54.0, radius);
        for (index, (tool, icon)) in actions.into_iter().enumerate() {
            let angle = std::f32::consts::PI * index as f32 / (actions.len() - 1) as f32;
            let point = center - vec2(radius * angle.cos(), rise * angle.sin());
            let opening = tool.and_then(Tool::opening);
            let sense = if opening.is_some() {
                Sense::click_and_drag()
            } else {
                Sense::click()
            };
            let label = tool.map_or("Формы", Tool::name);
            let response = icon.floating(
                ctx,
                ("build_tool", index),
                point,
                label,
                tool == Some(self.tool),
                sense,
            );
            if let Some(opening) = opening {
                response.dnd_set_drag_payload(opening);
            }
            if response.clicked() || response.drag_started() {
                self.reset_edit_state();
                self.build_open = false;
                if let Some(tool) = tool {
                    self.tool = tool;
                } else {
                    self.shape_at = Some(SHEET * 0.5);
                }
            }
        }
    }

    fn catalog(&mut self, ui: &mut egui::Ui, atlas: &FurnishingAtlas, mobile: bool) {
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            let accent = ui.visuals().hyperlink_color;
            ui.painter()
                .rect_filled(icon, 7.0, ui.visuals().selection.bg_fill);
            Destination::Catalog.icon(ui.painter(), icon.center(), Stroke::new(1.5_f32, accent));
            ui.label(RichText::new("Каталог").strong().size(17.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized([36.0, 32.0], egui::Button::new("×"))
                    .on_hover_text("Закрыть каталог")
                    .clicked()
                {
                    self.select_destination(Destination::Sheet);
                }
            });
        });
        if !self.catalog_open {
            return;
        }

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let width = ((ui.available_width() - 6.0) / 3.0).floor();
            ui.spacing_mut().button_padding.x = 3.0;
            for category in CatalogCategory::ALL {
                let button =
                    egui::Button::new(category.title()).selected(self.catalog_category == category);
                if ui.add_sized([width, 34.0], button).clicked() {
                    self.catalog_category = category;
                }
            }
        });
        ui.separator();

        let category = self.catalog_category;
        let height = if mobile {
            (ui.ctx().screen_rect().height() * 0.35).clamp(120.0, 235.0)
        } else {
            ui.available_height()
        };
        egui::ScrollArea::vertical()
            .id_salt(("catalog_items", category as u8))
            .max_height(height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for &tool in category.tools() {
                    self.catalog_row(ui, atlas, tool);
                }
                match category {
                    CatalogCategory::Furniture | CatalogCategory::Stairs => {
                        ui.checkbox(&mut self.rotated, "Повернуть при размещении");
                    }
                    CatalogCategory::Edit => self.catalog_edit_actions(ui, mobile),
                }
            });
        if category.tools().contains(&self.tool) || !self.message.is_empty() {
            self.tool_hint(ui);
        }
    }

    fn catalog_row(&mut self, ui: &mut egui::Ui, atlas: &FurnishingAtlas, tool: Tool) {
        let opening = tool.opening();
        let furnishing = tool.furnishing();
        let sense = if opening.is_some() {
            Sense::click_and_drag()
        } else {
            Sense::click()
        };
        let height = if furnishing.is_some() { 48.0 } else { 41.0 };
        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), sense);
        let color = ui.visuals().text_color();
        let selected = self.tool == tool;
        let fill = if selected {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().widgets.inactive.weak_bg_fill
        };
        ui.painter().rect_filled(rect, 10.0, fill);
        if response.hovered() || selected {
            let accent = ui.visuals().hyperlink_color;
            ui.painter()
                .rect_stroke(rect, 10.0, Stroke::new(1.0_f32, accent));
        }
        let text_x = if let Some(kind) = furnishing {
            let dimensions = kind.dimensions();
            let scale = (34.0 / dimensions.x).min(31.0 / dimensions.z);
            let icon = Rect::from_center_size(
                egui::pos2(rect.left() + 27.0, rect.center().y),
                vec2(dimensions.x * scale, dimensions.z * scale),
            );
            atlas.paint(
                ui.painter(),
                kind,
                [
                    icon.left_top(),
                    icon.right_top(),
                    icon.right_bottom(),
                    icon.left_bottom(),
                ],
            );
            rect.left() + 54.0
        } else {
            rect.left() + 13.0
        };
        ui.painter().text(
            egui::pos2(text_x, rect.center().y),
            Align2::LEFT_CENTER,
            tool.name(),
            FontId::proportional(14.0),
            color,
        );

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

    fn catalog_edit_actions(&mut self, ui: &mut egui::Ui, mobile: bool) {
        if self.tool == Tool::Pan {
            ui.horizontal_wrapped(|ui| {
                if ui.button("−").clicked() {
                    self.sheet_zoom = (self.sheet_zoom / 1.25).max(0.75);
                }
                if ui.button("+").clicked() {
                    self.sheet_zoom = (self.sheet_zoom * 1.25).min(5.0);
                }
                if ui.button("Вписать лист").clicked() {
                    self.fit_sheet();
                }
            });
        } else if self.tool == Tool::Edit && self.furniture_selection.is_some() {
            self.furniture_actions(ui);
        }
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.can_undo(), egui::Button::new("Отменить"))
                .clicked()
            {
                self.undo_plan(false);
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
                self.remember_wall_changes(previous);
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
            if self.room_edit == Some(index) || self.outline_room == Some(index) {
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
                    if on_sheet {
                        if let Some(point) = pointer {
                            let opening_selection = self.outline_action.is_none();
                            self.outline_tap(point);
                            if opening_selection {
                                self.outline_anchor = self.outline_bounds().map(|(a, b)| {
                                    Rect::from_two_pos(screen(a), screen(b)).right_bottom()
                                        + vec2(6.0, 6.0)
                                });
                            }
                        }
                    } else if self.outline_action.is_none() {
                        self.clear_selection();
                    }
                } else if response.drag_started() && self.outline_action.is_none() {
                    let start = ui
                        .input(|i| i.pointer.press_origin())
                        .filter(|point| rect.contains(*point))
                        .map(position);
                    if let Some(start) = start {
                        self.house_selection = HouseSelection::at(&self.plan, start);
                        if let (Some(house), Some(end)) = (&self.house_selection, pointer) {
                            self.outline_room = self.plan.room_at(start);
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
                            self.remember_wall_changes(previous);
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
                    self.edit_wall(index);
                } else {
                    self.room_edit = self.plan.room_at(point);
                }
                return;
            }
            Tool::Pencil | Tool::Pan | Tool::House => return,
            Tool::Erase => self.plan.erase(point),
            tool => {
                let Some(kind) = tool.furnishing() else {
                    return;
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
            if self.tool == Tool::Wall {
                self.remember_wall_changes(previous);
            } else {
                self.remember(previous);
            }
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
        editor.tool = Tool::Wall;
        editor.apply(Vec2::new(2.0, 2.0));
        editor.autosave_project().unwrap();
        assert!(!first.join("house.json").exists());
        editor.apply(Vec2::new(8.0, 2.0));
        editor.autosave_project().unwrap();
        assert!(!editor.plan_dirty);
        let mut editor = Editor::for_android(second.join("house.json"));
        assert!(editor.plan.walls.is_empty());
        assert_eq!(editor.photo_path, Some(second.join("plan-photo.gray")));
        editor.tool = Tool::Wall;
        editor.apply(Vec2::new(3.0, 3.0));
        editor.apply(Vec2::new(8.0, 3.0));
        editor.autosave_project().unwrap();
        assert_eq!(
            Editor::for_android(second.join("house.json"))
                .plan
                .walls
                .len(),
            1
        );
        editor.undo_plan(false);
        editor.autosave_project().unwrap();
        assert!(
            Editor::for_android(second.join("house.json"))
                .plan
                .walls
                .is_empty()
        );

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
    fn scene_plan_edits_are_available_on_the_sheet_and_undoable() {
        let mut editor = Editor::default();
        assert!(editor.plan.room(Vec2::new(4.0, 4.0), Vec2::new(12.0, 12.0)));
        editor.select_destination(Destination::Scene);
        editor.outline_tap(Vec2::new(6.0, 6.0));
        assert_eq!(editor.outline_room, Some(0));
        editor.outline_action = Some(OutlineAction::Wall);
        editor.outline_tap(Vec2::new(8.0, 4.0));
        editor.outline_tap(Vec2::new(8.0, 12.0));
        editor.select_destination(Destination::Sheet);
        assert_eq!(editor.plan.walls.len(), 5);
        assert_eq!(editor.plan.rooms.len(), 2);
        assert!(editor.outline_room.is_none());
        editor.undo_plan(false);
        assert_eq!(editor.plan.walls.len(), 4);
        assert_eq!(editor.plan.rooms.len(), 1);
    }

    #[test]
    fn bulk_undo_preserves_redo_order_and_a_new_edit_clears_redo() {
        let mut editor = Editor::default();
        for y in [2.0, 4.0] {
            let previous = editor.plan.clone();
            assert!(editor.plan.wall(Vec2::new(2.0, y), Vec2::new(6.0, y)));
            editor.remember(previous);
        }
        editor.undo_plan(true);
        assert!(editor.plan.walls.is_empty());
        editor.redo_plan();
        assert_eq!(editor.plan.walls.len(), 1);
        assert_eq!(editor.plan.walls[0].from[1], 2.0);
        editor.redo_plan();
        assert_eq!(editor.plan.walls.len(), 2);
        editor.undo_plan(false);
        let previous = editor.plan.clone();
        assert!(editor.plan.wall(Vec2::new(2.0, 6.0), Vec2::new(6.0, 6.0)));
        editor.remember(previous);
        assert!(editor.redo.is_empty());
    }

    #[test]
    fn catalog_lists_only_the_active_category_and_closes_from_its_header() {
        let ctx = egui::Context::default();
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&ctx, &atlas);
        let mut editor = Editor {
            catalog_open: true,
            catalog_category: CatalogCategory::Furniture,
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
        let text = |label| {
            layout.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(item) if item.galley.text() == label => {
                    Some(item.pos + item.galley.size() * 0.5)
                }
                _ => None,
            })
        };
        assert!(text("Кровать").is_some());
        assert!(text("Ворота").is_none());
        assert!(text("Лестница 90°").is_none());
        let close = text("×").expect("catalog has its own close button");
        let click = |pressed| egui::Event::PointerButton {
            pos: close,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(close), click(true)],
        );
        frame(&mut editor, vec![click(false), egui::Event::PointerGone]);
        assert!(!editor.catalog_open);
    }

    #[test]
    fn opening_survives_touch_pointer_gone_on_drop() {
        let ctx = egui::Context::default();
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&ctx, &atlas);
        let mut editor = Editor {
            build_open: true,
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
        frame(&mut editor, vec![]);
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
