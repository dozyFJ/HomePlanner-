use std::{fs, path::PathBuf};

use egui::{Align2, Color32, FontId, Frame, Pos2, Rect, Sense, Stroke, pos2, vec2};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use wolfcraft_engine::renderer::{FrameParams, model::ModelVertex};

const GROUND: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Cube {
    center: [f32; 3],
    size: f32,
    angle: f32,
}

impl Cube {
    fn center(self) -> Vec3 {
        Vec3::from_array(self.center) + Vec3::Y * GROUND
    }

    fn rotation(self) -> Quat {
        Quat::from_rotation_y(self.angle.to_radians())
    }

    fn corners(self) -> [glam::DVec3; 8] {
        let half = self.size * 0.5;
        std::array::from_fn(|index| {
            let sign = |bit: usize| {
                if index & (1_usize << bit) == 0_usize {
                    -half
                } else {
                    half
                }
            };
            (self.center() + self.rotation() * Vec3::new(sign(0), sign(1), sign(2))).as_dvec3()
        })
    }

    fn hit(self, origin: Vec3, direction: Vec3) -> Option<f32> {
        let inv = self.rotation().inverse();
        let local = inv * (origin - self.center());
        let ray = inv * direction;
        let half = self.size * 0.5;
        let mut near = 0.0_f32;
        let mut far = f32::INFINITY;
        for axis in 0..3 {
            if ray[axis].abs() < 1e-6 {
                if local[axis].abs() > half {
                    return None;
                }
                continue;
            }
            let a = (-half - local[axis]) / ray[axis];
            let b = (half - local[axis]) / ray[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
        Some(near)
    }

    fn valid(self) -> bool {
        self.center
            .iter()
            .all(|c| c.is_finite() && c.abs() <= 100.0)
            && (0.2..=20.0).contains(&self.size)
            && self.angle.is_finite()
            && self.angle.abs() <= 360.0
    }
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Orbit,
    Top,
    Front,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Orbit,
    Pan,
    Move,
}

struct Drag {
    index: usize,
    plane: Vec3,
    normal: Vec3,
    offset: Vec3,
    preview: Cube,
}

pub struct Modeling {
    cubes: Vec<Cube>,
    selected: Option<usize>,
    undo: Vec<Vec<Cube>>,
    path: PathBuf,
    message: String,
    view: View,
    mode: Mode,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: Vec3,
    drag: Option<Drag>,
}

impl Default for Modeling {
    fn default() -> Self {
        Self {
            cubes: vec![Cube {
                center: [0.0, 1.0, 0.0],
                size: 2.0,
                angle: 0.0,
            }],
            selected: Some(0),
            undo: vec![],
            path: PathBuf::from("model.json"),
            message: String::new(),
            view: View::Orbit,
            mode: Mode::Orbit,
            yaw: 0.68,
            pitch: 0.56,
            distance: 7.5,
            pan: Vec3::ZERO,
            drag: None,
        }
    }
}

impl Modeling {
    #[cfg(target_os = "android")]
    pub fn set_path(&mut self, path: PathBuf) {
        self.path = path;
        if self.path.is_file() {
            self.open();
        }
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        fs::write(&self.path, serde_json::to_vec_pretty(&self.cubes)?)?;
        Ok(())
    }

    fn open(&mut self) {
        let load = || -> Result<Vec<Cube>, Box<dyn std::error::Error>> {
            if fs::metadata(&self.path)?.len() > 96 * 1024 {
                return Err("Файл слишком большой".into());
            }
            let cubes: Vec<Cube> = serde_json::from_slice(&fs::read(&self.path)?)?;
            if cubes.len() > 100 || !cubes.iter().all(|cube| cube.valid()) {
                return Err("В модели есть недопустимые координаты или размеры".into());
            }
            Ok(cubes)
        };
        match load() {
            Ok(cubes) => {
                self.remember();
                self.cubes = cubes;
                self.selected = (!self.cubes.is_empty()).then_some(0);
                self.drag = None;
                self.focus();
                self.message = "Модель открыта".into();
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn remember(&mut self) {
        if self.undo.len() == 30 {
            self.undo.remove(0);
        }
        self.undo.push(self.cubes.clone());
    }

    fn focus(&mut self) {
        self.pan = self
            .selected
            .and_then(|index| self.cubes.get(index))
            .map_or(Vec3::ZERO, |cube| Vec3::from_array(cube.center) - Vec3::Y);
        self.distance = self
            .selected
            .and_then(|index| self.cubes.get(index))
            .map_or(7.5, |cube| (cube.size * 3.75).clamp(3.0, 50.0));
    }

    fn append(&mut self, cube: Cube) {
        if self.cubes.len() >= 100 {
            self.message = "В сцене может быть до 100 кубов".into();
            return;
        }
        self.remember();
        self.selected = Some(self.cubes.len());
        self.cubes.push(cube);
        self.message.clear();
    }

    fn update(&mut self, index: usize, cube: Cube) {
        if self.cubes.get(index).is_some_and(|old| *old != cube) {
            self.remember();
            self.cubes[index] = cube;
        }
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
        match view {
            View::Orbit => {
                self.yaw = 0.68;
                self.pitch = 0.56;
            }
            View::Top => {
                self.yaw = 0.0;
                self.pitch = std::f32::consts::FRAC_PI_2;
            }
            View::Front => {
                self.yaw = 0.0;
                self.pitch = 0.0;
            }
            View::Right => {
                self.yaw = std::f32::consts::FRAC_PI_2;
                self.pitch = 0.0;
            }
        }
    }

    fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(0.8, 90.0);
    }

    pub fn ui(&mut self, ctx: &egui::Context, mobile: bool) {
        let cancelled = ctx.input(|input| {
            input.key_pressed(egui::Key::Escape)
                || input.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Touch {
                            phase: egui::TouchPhase::Cancel,
                            ..
                        }
                    )
                })
        });
        if cancelled {
            self.drag = None;
        }
        egui::TopBottomPanel::bottom("model_tools")
            .frame(crate::theme::toolbar())
            .show(ctx, |ui| {
                if mobile {
                    ui.spacing_mut().interact_size.y = 31.0;
                }
                ui.horizontal_wrapped(|ui| {
                    for (mode, label) in [
                        (Mode::Orbit, "Орбита"),
                        (Mode::Pan, "Сдвиг"),
                        (Mode::Move, "Двигать куб"),
                    ] {
                        if ui.selectable_label(self.mode == mode, label).clicked() {
                            self.mode = mode;
                            self.drag = None;
                        }
                    }
                    if ui.button("−").clicked() {
                        self.zoom(1.18);
                    }
                    if ui.button("+").clicked() {
                        self.zoom(1.0 / 1.18);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    for (view, label) in [
                        (View::Orbit, "Перспектива"),
                        (View::Top, "Сверху"),
                        (View::Front, "Спереди"),
                        (View::Right, "Справа"),
                    ] {
                        if ui.selectable_label(self.view == view, label).clicked() {
                            self.set_view(view);
                        }
                    }
                    if ui.button("В кадр").clicked() {
                        self.focus();
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button("+ Куб").clicked() {
                        let x = self
                            .cubes
                            .iter()
                            .map(|cube| cube.center[0] + cube.size * 0.5)
                            .fold(-1.25, f32::max);
                        let cube = Cube {
                            center: [(x + 1.25).min(98.0), 1.0, 0.0],
                            size: 2.0,
                            angle: 0.0,
                        };
                        self.append(cube);
                        self.focus();
                    }
                    if let Some(index) = self.selected {
                        if ui.button("Копия").clicked() {
                            let mut copy = self.cubes[index];
                            copy.center[0] = (copy.center[0] + copy.size + 0.25).min(100.0);
                            self.append(copy);
                            self.focus();
                        }
                        if ui.button("Удалить").clicked() {
                            self.remember();
                            self.cubes.remove(index);
                            self.selected = None;
                        }
                    }
                    if ui
                        .add_enabled(!self.undo.is_empty(), egui::Button::new("Отменить"))
                        .clicked()
                    {
                        if let Some(cubes) = self.undo.pop() {
                            self.cubes = cubes;
                        }
                        self.selected = self.selected.filter(|index| *index < self.cubes.len());
                        self.drag = None;
                    }
                });
                if let Some(index) = self.selected {
                    let mut cube = self.cubes[index];
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Размер");
                        if ui.small_button("−").clicked() {
                            cube.size = (cube.size - 0.25).max(0.2);
                        }
                        ui.label(format!("{:.2} м", cube.size));
                        if ui.small_button("+").clicked() {
                            cube.size = (cube.size + 0.25).min(20.0);
                        }
                        if ui.button("↶ 15°").clicked() {
                            cube.angle = (cube.angle - 15.0).rem_euclid(360.0);
                        }
                        if ui.button("↷ 15°").clicked() {
                            cube.angle = (cube.angle + 15.0).rem_euclid(360.0);
                        }
                    });
                    if cube.size != self.cubes[index].size {
                        cube.center[1] = (cube.center[1]
                            + (cube.size - self.cubes[index].size) * 0.5)
                            .min(100.0);
                    }
                    self.update(index, cube);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Сохранить модель").clicked() {
                        self.message = self
                            .save()
                            .map(|_| "Модель сохранена".into())
                            .unwrap_or_else(|error| error.to_string());
                    }
                    if ui.button("Открыть модель").clicked() {
                        self.open();
                    }
                    if !self.message.is_empty() {
                        ui.label(&self.message);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                let screen = ctx.screen_rect();
                let camera = self.frame_params(screen.width() / screen.height().max(1.0), 0.0);
                let ray_at = |point| screen_ray(&camera, screen, point);
                let pointer = response.interact_pointer_pos();
                if !cancelled && response.clicked() {
                    self.selected = pointer.and_then(|point| self.pick(ray_at(point)));
                }
                let pinch = ctx.input(|input| input.multi_touch());
                if response.hovered() || response.dragged() {
                    if let Some(touch) = pinch {
                        self.drag = None;
                        self.zoom(1.0 / touch.zoom_delta.max(0.01));
                    } else {
                        let wheel = ctx.input(|input| input.raw_scroll_delta.y);
                        self.zoom((-wheel * 0.0015).exp());
                    }
                }
                if !cancelled && pinch.is_none() {
                    if self.mode == Mode::Move {
                        if response.drag_started() {
                            let start = ctx.input(|input| input.pointer.press_origin());
                            self.drag = start.and_then(|start| {
                                let ray = ray_at(start);
                                let index = self.pick(ray)?;
                                let cube = self.cubes[index];
                                let plane = cube.center();
                                let normal = match self.view {
                                    View::Front => Vec3::Z,
                                    View::Right => Vec3::X,
                                    _ => Vec3::Y,
                                };
                                let offset = plane - plane_hit(ray, plane, normal)?;
                                self.selected = Some(index);
                                Some(Drag {
                                    index,
                                    plane,
                                    normal,
                                    offset,
                                    preview: cube,
                                })
                            });
                        }
                        if let (Some(drag), Some(point)) = (&mut self.drag, pointer) {
                            if let Some(hit) = plane_hit(ray_at(point), drag.plane, drag.normal) {
                                let next = hit + drag.offset;
                                let world = next - Vec3::Y * GROUND;
                                let center = std::array::from_fn(|axis| {
                                    if drag.normal[axis] != 0.0 {
                                        drag.preview.center[axis]
                                    } else {
                                        ((world[axis] * 4.0).round() / 4.0).clamp(-100.0, 100.0)
                                    }
                                });
                                drag.preview.center = center;
                                ctx.request_repaint();
                            }
                        }
                        if response.drag_stopped() {
                            if let Some(drag) = self.drag.take() {
                                self.update(drag.index, drag.preview);
                            }
                        }
                    } else if response.dragged() {
                        let delta = response.drag_delta();
                        let pan =
                            self.mode == Mode::Pan || ctx.input(|input| input.modifiers.shift);
                        if pan {
                            let right = camera.forward.cross(camera.up).normalize();
                            let unit = self.distance * 0.0015;
                            self.pan += (-right * delta.x + camera.up * delta.y) * unit;
                        } else {
                            self.view = View::Orbit;
                            self.yaw -= delta.x * 0.009;
                            self.pitch = (self.pitch + delta.y * 0.009).clamp(-1.40, 1.52);
                        }
                    }
                }
                axes(ui.painter(), rect, &camera);
            });
    }

    fn pick(&self, (origin, ray): (Vec3, Vec3)) -> Option<usize> {
        self.cubes
            .iter()
            .enumerate()
            .filter_map(|(index, cube)| cube.hit(origin, ray).map(|distance| (index, distance)))
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(index, _)| index)
    }

    pub fn frame_params(&self, aspect: f32, time: f32) -> FrameParams {
        let (forward, up) = if self.view == View::Top {
            (Vec3::NEG_Y, Vec3::NEG_Z)
        } else {
            let outward = Vec3::new(
                self.yaw.sin() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.cos() * self.pitch.cos(),
            );
            let forward = -outward.normalize();
            (forward, forward.cross(Vec3::Y).normalize().cross(forward))
        };
        let target = Vec3::Y * (GROUND + 1.0) + self.pan;
        let eye = target - forward * (self.distance / aspect.clamp(0.45, 1.0));
        FrameParams {
            eye: eye.as_dvec3(),
            forward,
            up,
            fov_y: 48_f32.to_radians(),
            aspect,
            sun_dir: Vec3::new(0.5, 0.9, 0.3).normalize(),
            day: 1.0,
            underwater: false,
            view_distance_blocks: 230.0,
            time,
            outline: self.selected.and_then(|i| {
                self.cubes
                    .get(i)
                    .map(|cube| self.drawn_cube(i, *cube).corners())
            }),
            crack: 0.0,
            viewmodel_fov_y: 0.0,
        }
    }

    fn drawn_cube(&self, index: usize, cube: Cube) -> Cube {
        self.drag
            .as_ref()
            .filter(|drag| drag.index == index)
            .map_or(cube, |drag| drag.preview)
    }

    pub fn vertices(&self, eye: Vec3) -> Vec<ModelVertex> {
        let mut vertices = Vec::new();
        let y = GROUND - 0.004;
        let span = 30.0;
        for i in -(span as i32)..=span as i32 {
            let t = i as f32;
            let width = if i == 0 { 0.025 } else { 0.010 };
            let neutral = if i % 5 == 0 {
                [0.47, 0.52, 0.55]
            } else {
                [0.32, 0.38, 0.42]
            };
            let x_color = if i == 0 { [0.85, 0.24, 0.25] } else { neutral };
            let y_color = if i == 0 { [0.30, 0.72, 0.37] } else { neutral };
            let z = t;
            quad(
                &mut vertices,
                eye,
                [
                    Vec3::new(-span, y, z - width),
                    Vec3::new(-span, y, z + width),
                    Vec3::new(span, y, z + width),
                    Vec3::new(span, y, z - width),
                ],
                Vec3::Y,
                x_color,
            );
            let x = t;
            quad(
                &mut vertices,
                eye,
                [
                    Vec3::new(x - width, y + 0.001, -span),
                    Vec3::new(x - width, y + 0.001, span),
                    Vec3::new(x + width, y + 0.001, span),
                    Vec3::new(x + width, y + 0.001, -span),
                ],
                Vec3::Y,
                y_color,
            );
        }
        for (index, &cube) in self.cubes.iter().enumerate() {
            let cube = self.drawn_cube(index, cube);
            let rotation = cube.rotation();
            let axes = [rotation * Vec3::X, Vec3::Y, rotation * Vec3::Z];
            let half = cube.size * 0.5;
            for (axis, u, v) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
                for sign in [-1.0, 1.0] {
                    let normal = axes[axis] * sign;
                    let along = axes[u] * half * sign;
                    let across = axes[v] * half;
                    let c = cube.center() + normal * half;
                    quad(
                        &mut vertices,
                        eye,
                        [
                            c - along - across,
                            c + along - across,
                            c + along + across,
                            c - along + across,
                        ],
                        normal,
                        [0.62, 0.68, 0.72],
                    );
                }
            }
        }
        vertices
    }
}

fn quad(
    vertices: &mut Vec<ModelVertex>,
    eye: Vec3,
    corners: [Vec3; 4],
    normal: Vec3,
    color: [f32; 3],
) {
    for i in [0, 1, 2, 0, 2, 3] {
        vertices.push(ModelVertex {
            pos: (corners[i] - eye).to_array(),
            normal: normal.to_array(),
            color,
            uv: [3.0, 2.0],
            tex: 0.0,
        });
    }
}

fn screen_ray(camera: &FrameParams, screen: Rect, point: Pos2) -> (Vec3, Vec3) {
    let right = camera.forward.cross(camera.up).normalize();
    let tangent = (camera.fov_y * 0.5).tan();
    let x =
        (2.0 * (point.x - screen.left()) / screen.width().max(1.0) - 1.0) * camera.aspect * tangent;
    let y = (1.0 - 2.0 * (point.y - screen.top()) / screen.height().max(1.0)) * tangent;
    (
        camera.eye.as_vec3(),
        (camera.forward + right * x + camera.up * y).normalize(),
    )
}

fn plane_hit((origin, ray): (Vec3, Vec3), point: Vec3, normal: Vec3) -> Option<Vec3> {
    let alignment = ray.dot(normal);
    if alignment.abs() < 0.001 {
        return None;
    }
    let distance = (point - origin).dot(normal) / alignment;
    (distance > 0.0).then_some(origin + ray * distance)
}

fn axes(painter: &egui::Painter, rect: Rect, camera: &FrameParams) {
    let center = pos2(rect.right() - 35.0, rect.top() + 35.0);
    let right = camera.forward.cross(camera.up).normalize();
    for (axis, name, color) in [
        (Vec3::X, "X", Color32::from_rgb(230, 80, 87)),
        (Vec3::Z, "Y", Color32::from_rgb(95, 198, 110)),
        (Vec3::Y, "Z", Color32::from_rgb(86, 157, 240)),
    ] {
        let offset = vec2(axis.dot(right), -axis.dot(camera.up)) * 20.0;
        if offset.length_sq() > 3.0 {
            let end = center + offset;
            painter.line_segment([center, end], Stroke::new(2.0_f32, color));
            painter.text(
                end,
                Align2::CENTER_CENTER,
                name,
                FontId::monospace(13.0),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_cube_is_pickable_from_every_preset() {
        let mut model = Modeling::default();
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(600.0, 800.0));
        for view in [View::Orbit, View::Top, View::Front, View::Right] {
            model.set_view(view);
            let frame = model.frame_params(0.75, 0.0);
            assert_eq!(model.pick(screen_ray(&frame, rect, rect.center())), Some(0));
            assert!(frame.outline.is_some());
        }
    }
}
