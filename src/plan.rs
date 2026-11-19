use glam::{Vec2, Vec3};
use serde::{Deserialize, Serialize};
use wolfcraft_engine::renderer::model::ModelVertex;

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
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Area {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OpeningKind {
    Door,
    Window,
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
}

#[derive(Clone, Copy, Serialize, Deserialize)]
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
        }
    }
}

impl Furniture {
    pub fn size(self) -> Vec3 {
        let size = match self.kind {
            FurnitureKind::Bed => Vec3::new(1.6, 0.48, 2.0),
            FurnitureKind::Table => Vec3::new(1.4, 0.75, 0.85),
            FurnitureKind::Sofa => Vec3::new(2.1, 0.78, 0.85),
        };
        if self.rotated {
            Vec3::new(size.z, size.y, size.x)
        } else {
            size
        }
    }

    pub fn bounds(self) -> (Vec2, Vec2) {
        let center = Vec2::from_array(self.at);
        let half = self.size().xz() * 0.5;
        (center - half, center + half)
    }
}

use glam::Vec3Swizzles;

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

    fn add_opening(&mut self, kind: OpeningKind, at: f32) -> bool {
        let (a, b) = self.points();
        let half = kind.width() * 0.5;
        if at < half
            || at + half > a.distance(b)
            || self
                .openings
                .iter()
                .any(|other| (other.at - at).abs() < half + other.kind.width() * 0.5)
        {
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
        });
        true
    }

    fn nearest_wall(&self, point: Vec2) -> Option<(usize, f32)> {
        self.walls
            .iter()
            .enumerate()
            .map(|(index, wall)| (index, wall.projection(point)))
            .filter(|(_, (_, distance))| *distance <= 0.4)
            .min_by(|(_, (_, a)), (_, (_, b))| a.total_cmp(b))
            .map(|(index, (at, _))| (index, at))
    }

    pub fn opening(&mut self, kind: OpeningKind, point: Vec2) -> bool {
        self.nearest_wall(point)
            .is_some_and(|(index, at)| self.walls[index].add_opening(kind, at))
    }

    pub fn erase(&mut self, point: Vec2) -> bool {
        if let Some(index) = self.furniture.iter().rposition(|item| {
            let (a, b) = item.bounds();
            point.cmpge(a).all() && point.cmple(b).all()
        }) {
            self.furniture.remove(index);
        } else if let Some((index, at)) = self.nearest_wall(point) {
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
        } else if let Some(index) = self.rooms.iter().rposition(|area| {
            point.cmpge(Vec2::from_array(area.min)).all()
                && point.cmple(Vec2::from_array(area.max)).all()
        }) {
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

pub fn model(plan: &Plan, eye: Vec3, cutaway: bool) -> Vec<ModelVertex> {
    let mut output = Vec::new();
    let mut prism = |center: Vec2, size: Vec3, y: f32, dir: Vec2, color: [f32; 3]| {
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
                        uv: [3.0, 2.0],
                        tex: 0.0,
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
            [0.81, 0.68, 0.51],
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
                    Vec3::new(end - start, top - bottom, 0.18),
                    (bottom + top) * 0.5,
                    dir,
                    [0.89, 0.88, 0.8],
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
        let size = item.size();
        let color = match item.kind {
            FurnitureKind::Bed => [0.39, 0.62, 0.70],
            FurnitureKind::Table => [0.62, 0.40, 0.25],
            FurnitureKind::Sofa => [0.61, 0.42, 0.50],
        };
        prism(
            Vec2::from_array(item.at),
            size,
            size.y * 0.5 + 0.01,
            Vec2::X,
            color,
        );
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn preview_keeps_a_door_clear_below_its_lintel() {
        let mut plan = Plan::default();
        assert!(plan.wall(Vec2::new(2.0, 2.0), Vec2::new(8.0, 2.0)));
        assert!(plan.opening(OpeningKind::Door, Vec2::new(4.0, 2.0)));
        let vertices = model(&plan, Vec3::ZERO, false);
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
