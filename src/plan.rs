use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use wolfcraft_assets::UvRect;
use wolfcraft_engine::renderer::model::ModelVertex;

use crate::furnishings::{FurnishingAtlas, uv_corners};

pub const SHEET: Vec2 = Vec2::new(24.0, 16.0);

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Plan {
    pub walls: Vec<Wall>,
    pub rooms: Vec<Area>,
    pub furniture: Vec<Furniture>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Wall {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub openings: Vec<Opening>,
    #[serde(default = "default_wall_thickness")]
    pub thickness: f32,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Area {
    pub min: [f32; 2],
    pub max: [f32; 2],
    #[serde(default)]
    pub finish: FloorFinish,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum FloorFinish {
    #[default]
    Sand,
    Walnut,
    Stone,
    Sage,
}

impl FloorFinish {
    pub const ALL: [(Self, &'static str); 4] = [
        (Self::Sand, "Песочный"),
        (Self::Walnut, "Орех"),
        (Self::Stone, "Серый камень"),
        (Self::Sage, "Шалфей"),
    ];

    pub fn color(self) -> [f32; 3] {
        match self {
            Self::Sand => [0.81, 0.68, 0.51],
            Self::Walnut => [0.56, 0.35, 0.25],
            Self::Stone => [0.63, 0.67, 0.71],
            Self::Sage => [0.56, 0.68, 0.52],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OpeningKind {
    Door,
    Window,
    Garage,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Opening {
    pub at: f32,
    pub kind: OpeningKind,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FurnitureKind {
    Bed,
    Table,
    Sofa,
    Cabinet,
    Lamp,
    Stair,
    StairL,
    StairU,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Furniture {
    pub at: [f32; 2],
    pub kind: FurnitureKind,
    pub rotated: bool,
}

impl OpeningKind {
    pub fn width(self) -> f32 {
        match self {
            Self::Door => 0.9,
            Self::Window => 1.2,
            Self::Garage => 2.6,
        }
    }
}

impl Furniture {
    pub fn size(self) -> Vec3 {
        let size = self.kind.dimensions();
        if self.rotated {
            Vec3::new(size.z, size.y, size.x)
        } else {
            size
        }
    }

    pub fn point(self, local: Vec2) -> Vec2 {
        let offset = if self.rotated {
            Vec2::new(-local.y, local.x)
        } else {
            local
        };
        Vec2::from_array(self.at) + offset
    }

    pub fn bounds(self) -> (Vec2, Vec2) {
        let center = Vec2::from_array(self.at);
        let half = self.size().xz() * 0.5;
        (center - half, center + half)
    }
}

use glam::Vec3Swizzles;

fn default_wall_thickness() -> f32 {
    0.18
}

pub struct OpeningPlacement {
    wall: usize,
    at: f32,
    kind: OpeningKind,
    pub span: [Vec2; 2],
}

impl Wall {
    pub fn points(&self) -> (Vec2, Vec2) {
        (Vec2::from_array(self.from), Vec2::from_array(self.to))
    }

    pub fn projection(&self, point: Vec2) -> (f32, f32) {
        let (a, b) = self.points();
        let edge = b - a;
        let distance = (point - a)
            .dot(edge.normalize_or_zero())
            .clamp(0.0, edge.length());
        (
            distance,
            (point - (a + edge.normalize_or_zero() * distance)).length(),
        )
    }

    fn can_place(&self, kind: OpeningKind, at: f32) -> bool {
        let (a, b) = self.points();
        let half = kind.width() * 0.5;
        at >= half
            && at + half <= a.distance(b)
            && self
                .openings
                .iter()
                .all(|other| (other.at - at).abs() >= half + other.kind.width() * 0.5)
    }

    fn add_opening(&mut self, kind: OpeningKind, at: f32) -> bool {
        if !self.can_place(kind, at) {
            return false;
        }
        self.openings.push(Opening { kind, at });
        self.openings.sort_by(|a, b| a.at.total_cmp(&b.at));
        true
    }
}

impl Plan {
    pub fn wall(&mut self, a: Vec2, b: Vec2) -> bool {
        if a.distance(b) < 0.24 {
            return false;
        }
        let (from, to) = (a.to_array(), b.to_array());
        if self
            .walls
            .iter()
            .any(|w| (w.from == from && w.to == to) || (w.from == to && w.to == from))
        {
            return false;
        }
        self.walls.push(Wall {
            from,
            to,
            openings: Vec::new(),
            thickness: default_wall_thickness(),
        });
        true
    }

    pub fn room(&mut self, a: Vec2, b: Vec2) -> bool {
        let min = a.min(b);
        let max = a.max(b);
        if (max - min).min_element() < 0.5 {
            return false;
        }
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        for index in 0..4 {
            self.wall(corners[index], corners[(index + 1) % 4]);
        }
        self.rooms.push(Area {
            min: min.to_array(),
            max: max.to_array(),
            finish: FloorFinish::default(),
        });
        true
    }

    pub fn split_room(&mut self, index: usize, axis: usize, at: Option<f32>) -> Option<usize> {
        let room = *self.rooms.get(index)?;
        let coordinate = at.unwrap_or((room.min[axis] + room.max[axis]) * 0.5);
        let middle = (coordinate * 2.0).round() * 0.5;
        if middle - room.min[axis] < 0.5 || room.max[axis] - middle < 0.5 {
            return None;
        }

        let along = 1 - axis;
        let mut start = room.min;
        let mut end = room.max;
        start[axis] = middle;
        end[axis] = middle;
        let mut spans: Vec<_> = self
            .walls
            .iter()
            .filter_map(|wall| {
                if (wall.from[axis] - middle).abs() > 0.001
                    || (wall.to[axis] - middle).abs() > 0.001
                {
                    return None;
                }
                let lo = wall.from[along].min(wall.to[along]).max(start[along]);
                let hi = wall.from[along].max(wall.to[along]).min(end[along]);
                (lo < hi).then_some((lo, hi))
            })
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        spans.push((end[along], end[along]));
        for (lo, hi) in spans {
            if lo > start[along] {
                let mut gap_end = end;
                gap_end[along] = lo;
                self.wall(Vec2::from_array(start), Vec2::from_array(gap_end));
            }
            start[along] = start[along].max(hi);
        }

        let mut first = room;
        let mut second = room;
        first.max[axis] = middle;
        second.min[axis] = middle;
        self.rooms[index] = first;
        let next = self.rooms.len();
        self.rooms.push(second);
        Some(next)
    }

    pub fn l_room(&mut self, origin: Vec2) {
        let outline = [
            Vec2::ZERO,
            Vec2::new(8.0, 0.0),
            Vec2::new(8.0, 4.0),
            Vec2::new(4.0, 4.0),
            Vec2::new(4.0, 8.0),
            Vec2::new(0.0, 8.0),
        ];
        for (index, point) in outline.into_iter().enumerate() {
            self.wall(
                origin + point,
                origin + outline[(index + 1) % outline.len()],
            );
        }
        for (a, b) in [
            (Vec2::ZERO, Vec2::new(8.0, 4.0)),
            (Vec2::new(0.0, 4.0), Vec2::new(4.0, 8.0)),
        ] {
            self.rooms.push(Area {
                min: (origin + a).to_array(),
                max: (origin + b).to_array(),
                finish: FloorFinish::default(),
            });
        }
    }

    pub fn preview_bounds(&self) -> (Vec2, Vec2) {
        let mut min = SHEET;
        let mut max = Vec2::ZERO;
        let mut include = |a: Vec2, b: Vec2| {
            min = min.min(a).min(b);
            max = max.max(a).max(b);
        };
        for wall in &self.walls {
            let (a, b) = wall.points();
            include(a, b);
        }
        for area in &self.rooms {
            include(Vec2::from_array(area.min), Vec2::from_array(area.max));
        }
        for item in &self.furniture {
            let (a, b) = item.bounds();
            include(a, b);
        }
        if min.cmpgt(max).any() {
            (Vec2::ZERO, SHEET)
        } else {
            (min, max)
        }
    }

    pub fn nearest_wall(&self, point: Vec2, tolerance: f32) -> Option<(usize, f32)> {
        self.walls
            .iter()
            .enumerate()
            .map(|(index, wall)| (index, wall.projection(point)))
            .filter(|(_, (_, distance))| *distance <= tolerance)
            .min_by(|(_, (_, a)), (_, (_, b))| a.total_cmp(b))
            .map(|(index, (at, _))| (index, at))
    }

    pub fn opening(&mut self, kind: OpeningKind, point: Vec2) -> bool {
        self.nearest_wall(point, 0.4)
            .is_some_and(|(index, at)| self.walls[index].add_opening(kind, at))
    }

    pub fn snap_opening(
        &self,
        kind: OpeningKind,
        point: Vec2,
        tolerance: f32,
    ) -> Option<OpeningPlacement> {
        let half = kind.width() * 0.5;
        self.walls
            .iter()
            .enumerate()
            .filter_map(|(index, wall)| {
                let (projection, distance) = wall.projection(point);
                let (a, b) = wall.points();
                let length = a.distance(b);
                if distance > tolerance || length < kind.width() {
                    return None;
                }
                let at = projection.clamp(half, length - half);
                let dir = (b - a).normalize();
                let center = a + dir * at;
                let drift = point.distance(center);
                if drift > tolerance || !wall.can_place(kind, at) {
                    return None;
                }
                let span = [center - dir * half, center + dir * half];
                Some((
                    drift,
                    OpeningPlacement {
                        wall: index,
                        at,
                        kind,
                        span,
                    },
                ))
            })
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, placement)| placement)
    }

    pub fn attach_opening(&mut self, placement: OpeningPlacement) -> bool {
        self.walls
            .get_mut(placement.wall)
            .is_some_and(|wall| wall.add_opening(placement.kind, placement.at))
    }

    pub fn resize_wall(&mut self, index: usize, length: f32, thickness: f32) -> bool {
        let Some(wall) = self.walls.get_mut(index) else {
            return false;
        };
        let (start, end) = wall.points();
        if !length.is_finite()
            || !(0.24..=SHEET.length()).contains(&length)
            || !(0.08..=0.80).contains(&thickness)
            || wall
                .openings
                .iter()
                .any(|other| other.at + other.kind.width() * 0.5 > length)
        {
            return false;
        }
        let end_next = start + (end - start).normalize() * length;
        if !end_next.cmpge(-Vec2::splat(0.0001)).all()
            || !end_next.cmple(SHEET + Vec2::splat(0.0001)).all()
        {
            return false;
        }
        wall.to = end_next.clamp(Vec2::ZERO, SHEET).to_array();
        wall.thickness = thickness;
        true
    }

    pub fn furniture_at(&self, point: Vec2) -> Option<usize> {
        self.furniture.iter().rposition(|item| {
            let (a, b) = item.bounds();
            point.cmpge(a).all() && point.cmple(b).all()
        })
    }

    pub fn room_at(&self, point: Vec2) -> Option<usize> {
        self.rooms.iter().rposition(|area| {
            point.cmpge(Vec2::from_array(area.min)).all()
                && point.cmple(Vec2::from_array(area.max)).all()
        })
    }

    pub fn erase(&mut self, point: Vec2) -> bool {
        if let Some(index) = self.furniture_at(point) {
            self.furniture.remove(index);
        } else if let Some((index, at)) = self.nearest_wall(point, 0.4) {
            let wall = &mut self.walls[index];
            if let Some(opening) = wall
                .openings
                .iter()
                .position(|o| (o.at - at).abs() < o.kind.width() * 0.5)
            {
                wall.openings.remove(opening);
            } else {
                self.walls.remove(index);
            }
        } else if let Some(index) = self.room_at(point) {
            self.rooms.remove(index);
        } else {
            return false;
        }
        true
    }

    pub fn valid(&self) -> bool {
        let point = |p: [f32; 2]| {
            let p = Vec2::from_array(p);
            p.is_finite() && p.cmpge(Vec2::ZERO).all() && p.cmple(SHEET).all()
        };
        self.walls.len() <= 2000
            && self.rooms.len() <= 2000
            && self.furniture.len() <= 2000
            && self.walls.iter().all(|w| {
                let (a, b) = w.points();
                let mut end = 0.0;
                point(w.from)
                    && point(w.to)
                    && a.distance(b) >= 0.24
                    && (0.08..=0.80).contains(&w.thickness)
                    && w.openings.iter().all(|o| {
                        let half = o.kind.width() / 2.0;
                        let fits = o.at - half >= end && o.at + half <= a.distance(b);
                        end = o.at + half;
                        fits
                    })
            })
            && self.rooms.iter().all(|r| {
                point(r.min)
                    && point(r.max)
                    && (Vec2::from_array(r.max) - Vec2::from_array(r.min)).min_element() >= 0.5
            })
            && self.furniture.iter().all(|f| point(f.at))
    }
}

pub fn model(
    plan: &Plan,
    eye: Vec3,
    cutaway: bool,
    atlas: Option<&FurnishingAtlas>,
) -> Vec<ModelVertex> {
    let mut output = Vec::new();
    let mut prism =
        |center: Vec2, size: Vec3, y: f32, dir: Vec2, color: [f32; 3], texture: Option<UvRect>| {
            let uv = texture.map_or([[3.0, 2.0]; 4], uv_corners);
            let tex = if texture.is_some() { 1.0 } else { 0.0 };
            let x = Vec3::new(dir.x, 0.0, dir.y);
            let z = x.cross(Vec3::Y);
            let origin = Vec3::new(center.x, 40.0 + y, center.y) - eye;
            let axes = [x, Vec3::Y, z];
            let half = size * 0.5;
            for (axis, u, v) in [(0, 1, 2), (1, 2, 0), (2, 0, 1)] {
                for sign in [-1.0, 1.0] {
                    let normal = axes[axis] * sign;
                    let along = axes[u] * half[u] * sign;
                    let across = axes[v] * half[v];
                    let c = origin + normal * half[axis];
                    let corners = [
                        c - along - across,
                        c + along - across,
                        c + along + across,
                        c - along + across,
                    ];
                    for index in [0, 1, 2, 0, 2, 3] {
                        output.push(ModelVertex {
                            pos: corners[index].to_array(),
                            normal: normal.to_array(),
                            color,
                            uv: uv[index],
                            tex,
                        });
                    }
                }
            }
        };
    prism(
        SHEET * 0.5,
        Vec3::new(SHEET.x, 0.18, SHEET.y),
        -0.24,
        Vec2::X,
        [0.33, 0.43, 0.44],
        None,
    );
    for (index, area) in plan.rooms.iter().enumerate() {
        let a = Vec2::from_array(area.min);
        let b = Vec2::from_array(area.max);
        let size = b - a;
        prism(
            (a + b) * 0.5,
            Vec3::new(size.x, 0.14, size.y),
            -0.07 + index as f32 * 0.00001,
            Vec2::X,
            area.finish.color(),
            None,
        );
    }
    for wall in &plan.walls {
        let (a, b) = wall.points();
        let dir = (b - a).normalize();
        let height = if cutaway { 1.0 } else { 2.65 };
        let mut section = |start: f32, end: f32, bottom: f32, top: f32| {
            if end > start && top > bottom {
                prism(
                    a + dir * (start + end) * 0.5,
                    Vec3::new(end - start, top - bottom, wall.thickness),
                    (bottom + top) * 0.5,
                    dir,
                    [0.89, 0.88, 0.8],
                    None,
                );
            }
        };
        let mut cursor = 0.0;
        for o in &wall.openings {
            let start = o.at - o.kind.width() * 0.5;
            let end = o.at + o.kind.width() * 0.5;
            section(cursor, start, 0.0, height);
            match o.kind {
                OpeningKind::Door => section(start, end, 2.10, height),
                OpeningKind::Garage => section(start, end, 2.25, height),
                OpeningKind::Window => {
                    section(start, end, 0.0, 0.85_f32.min(height));
                    section(start, end, 2.05, height);
                }
            }
            cursor = end;
        }
        section(cursor, a.distance(b), 0.0, height);
    }
    for item in &plan.furniture {
        let axis = if item.rotated { Vec2::Y } else { Vec2::X };
        for part in item.kind.parts() {
            prism(
                item.point(part.position.xz()),
                part.size,
                part.position.y + 0.01,
                axis,
                [0.65; 3],
                atlas.map(|atlas| atlas.material(part.material)),
            );
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_room_preserves_the_perimeter_and_existing_partition_opening() {
        let mut plan = Plan::default();
        assert!(plan.room(Vec2::ZERO, Vec2::new(8.0, 6.0)));
        plan.rooms[0].finish = FloorFinish::Sage;
        assert!(plan.wall(Vec2::new(4.0, 0.0), Vec2::new(4.0, 4.0)));
        assert!(plan.opening(OpeningKind::Door, Vec2::new(4.0, 2.0)));
        assert_eq!(plan.split_room(0, 0, Some(4.1)), Some(1));
        assert_eq!(plan.rooms[0].max, [4.0, 6.0]);
        assert_eq!(plan.rooms[1].min, [4.0, 0.0]);
        assert_eq!(plan.rooms[1].finish, FloorFinish::Sage);
        assert_eq!(plan.walls.len(), 6);
        assert_eq!(plan.walls[4].openings.len(), 1);
        assert_eq!(
            plan.walls[5].points(),
            (Vec2::new(4.0, 4.0), Vec2::new(4.0, 6.0))
        );
    }

    #[test]
    fn openings_fit_the_wall_and_do_not_overlap() {
        let mut plan = Plan::default();
        assert!(plan.wall(Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0)));
        assert!(plan.opening(OpeningKind::Door, Vec2::new(4.0, 2.1)));
        assert!(!plan.opening(OpeningKind::Window, Vec2::new(4.3, 2.0)));
        assert!(!plan.opening(OpeningKind::Door, Vec2::new(2.15, 2.0)));
        assert!(plan.opening(OpeningKind::Window, Vec2::new(6.0, 2.0)));
        let saved = serde_json::to_string(&plan).unwrap();
        let restored: Plan = serde_json::from_str(&saved).unwrap();
        assert!(restored.valid());
        assert_eq!(restored.walls[0].openings.len(), 2);
    }

    #[test]
    fn garage_drop_snaps_and_wall_edit_preserves_the_opening() {
        let mut plan = Plan::default();
        assert!(plan.wall(Vec2::new(2.0, 2.0), Vec2::new(10.0, 2.0)));
        let gate = plan
            .snap_opening(OpeningKind::Garage, Vec2::new(6.0, 2.7), 0.9)
            .unwrap();
        assert!((gate.span[0].distance(gate.span[1]) - 2.6).abs() < 0.0001);
        assert_eq!(gate.span[0].y, 2.0);
        assert!(plan.attach_opening(gate));
        assert!(
            plan.snap_opening(OpeningKind::Window, Vec2::new(6.0, 2.2), 0.9)
                .is_none()
        );
        assert!(!plan.resize_wall(0, 5.0, 0.25));
        assert_eq!(plan.walls[0].thickness, 0.18);
        assert!(plan.resize_wall(0, 6.0, 0.25));
        assert_eq!(plan.walls[0].to, [8.0, 2.0]);
        assert_eq!(plan.walls[0].thickness, 0.25);
        assert!(plan.valid());
    }

    #[test]
    fn saved_legacy_wall_uses_default_thickness() {
        let wall: Wall =
            serde_json::from_str(r#"{"from":[2.0,2.0],"to":[8.0,2.0],"openings":[]}"#).unwrap();
        assert_eq!(wall.thickness, 0.18);
    }

    #[test]
    fn floor_finish_keeps_legacy_projects_and_reaches_3d() {
        let area: Area = serde_json::from_str(r#"{"min":[2.0,2.0],"max":[6.0,5.0]}"#).unwrap();
        assert_eq!(area.finish, FloorFinish::Sand);
        let mut plan = Plan {
            rooms: vec![area],
            ..Default::default()
        };
        plan.rooms[0].finish = FloorFinish::Sage;
        let saved = serde_json::to_string(&plan).unwrap();
        let loaded: Plan = serde_json::from_str(&saved).unwrap();
        assert_eq!(loaded.rooms[0].finish, FloorFinish::Sage);
        let mesh = model(&loaded, Vec3::ZERO, false, None);
        assert!(
            mesh[36..]
                .iter()
                .all(|vertex| vertex.color == FloorFinish::Sage.color())
        );
    }

    #[test]
    fn l_preset_leaves_the_inner_corner_open() {
        let mut plan = Plan::default();
        plan.l_room(Vec2::new(2.0, 2.0));
        assert!(plan.valid());
        assert_eq!(plan.walls.len(), 6);
        let area = plan
            .rooms
            .iter()
            .map(|room| {
                let size = Vec2::from_array(room.max) - Vec2::from_array(room.min);
                size.x * size.y
            })
            .sum::<f32>();
        assert_eq!(area, 48.0);
        assert!(
            plan.rooms
                .iter()
                .all(|room| room.max[0] <= 6.0 || room.max[1] <= 6.0)
        );
    }

    #[test]
    fn atlas_materials_are_used_only_by_placed_furnishings() {
        let atlas = FurnishingAtlas::build().unwrap();
        let atlas = FurnishingAtlas::upload(&egui::Context::default(), &atlas);
        let mut plan = Plan::default();
        assert!(
            model(&plan, Vec3::ZERO, false, Some(&atlas))
                .iter()
                .all(|v| v.tex == 0.0)
        );
        let furniture = Furniture {
            at: [10.0, 8.0],
            kind: FurnitureKind::Table,
            rotated: true,
        };
        let (min, max) = furniture.bounds();
        plan.furniture.push(furniture);
        let vertices = model(&plan, Vec3::ZERO, false, Some(&atlas));
        let textured: Vec<_> = vertices.iter().filter(|v| v.tex == 1.0).collect();
        assert_eq!(textured.len(), FurnitureKind::Table.parts().len() * 36);
        assert!(textured.iter().all(|vertex| {
            let pos = Vec3::from_array(vertex.pos).xz();
            pos.cmpge(min - Vec2::splat(0.001)).all() && pos.cmple(max + Vec2::splat(0.001)).all()
        }));
    }

    #[test]
    fn preview_keeps_a_door_clear_below_its_lintel() {
        let mut plan = Plan::default();
        assert!(plan.wall(Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0)));
        assert!(plan.opening(OpeningKind::Door, Vec2::new(4.0, 2.0)));
        let vertices = model(&plan, Vec3::ZERO, false, None);
        assert_eq!(vertices.len(), 4 * 36); // Ground, wall on both sides, and lintel.
        for triangle in vertices.chunks_exact(3).skip(12) {
            let [a, b, c] = triangle else { unreachable!() };
            let a = Vec3::from_array(a.pos);
            let b = Vec3::from_array(b.pos);
            let c = Vec3::from_array(c.pos);
            let normal = Vec3::from_array(triangle[0].normal);
            assert!((b - a).cross(c - a).dot(normal) > 0.0);
            let center = (a + b + c) / 3.0;
            assert!(center.x <= 3.551 || center.x >= 4.449 || center.y >= 42.099);
        }
    }
}
