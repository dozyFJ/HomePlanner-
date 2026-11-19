use std::collections::VecDeque;

use glam::Vec2;

use crate::plan::{Area, Plan, SHEET, Wall};

pub struct HouseSelection {
    pub walls: Vec<bool>,
    pub rooms: Vec<bool>,
    pub furniture: Vec<bool>,
    pub min: Vec2,
    pub max: Vec2,
}

#[derive(Clone, Copy)]
enum Part {
    Wall(usize),
    Room(usize),
}

impl HouseSelection {
    pub fn at(plan: &Plan, point: Vec2) -> Option<Self> {
        let seed = plan.room_at(point).map(Part::Room).or_else(|| {
            plan.nearest_wall(point, 0.60)
                .map(|(index, _)| Part::Wall(index))
        })?;
        let mut selection = Self {
            walls: vec![false; plan.walls.len()],
            rooms: vec![false; plan.rooms.len()],
            furniture: vec![false; plan.furniture.len()],
            min: SHEET,
            max: Vec2::ZERO,
        };
        let mut queue = VecDeque::new();
        selection.enqueue(seed, &mut queue);
        while let Some(current) = queue.pop_front() {
            match current {
                Part::Wall(index) => {
                    let w = &plan.walls[index];
                    let (a, b) = w.points();
                    selection.include(a, b);
                    for (i, other) in plan.walls.iter().enumerate() {
                        if !selection.walls[i] && walls_meet(w, other) {
                            selection.enqueue(Part::Wall(i), &mut queue);
                        }
                    }
                    for (i, room) in plan.rooms.iter().enumerate() {
                        if !selection.rooms[i] && wall_touches(w, room) {
                            selection.enqueue(Part::Room(i), &mut queue);
                        }
                    }
                }
                Part::Room(index) => {
                    let area = &plan.rooms[index];
                    let min = Vec2::from_array(area.min);
                    let max = Vec2::from_array(area.max);
                    selection.include(min, max);
                    for (i, room) in plan.rooms.iter().enumerate() {
                        if !selection.rooms[i]
                            && min.cmple(Vec2::from_array(room.max)).all()
                            && Vec2::from_array(room.min).cmple(max).all()
                        {
                            selection.enqueue(Part::Room(i), &mut queue);
                        }
                    }
                    for (i, wall) in plan.walls.iter().enumerate() {
                        if !selection.walls[i] && wall_touches(wall, area) {
                            selection.enqueue(Part::Wall(i), &mut queue);
                        }
                    }
                }
            }
        }
        let has_rooms = selection.rooms.iter().any(|&chosen| chosen);
        let (min, max) = (selection.min, selection.max);
        for (index, item) in plan.furniture.iter().enumerate() {
            let at = Vec2::from_array(item.at);
            let in_area = plan
                .rooms
                .iter()
                .zip(&selection.rooms)
                .any(|(room, chosen)| {
                    *chosen
                        && at.cmpge(Vec2::from_array(room.min)).all()
                        && at.cmple(Vec2::from_array(room.max)).all()
                });
            if in_area || (!has_rooms && at.cmpge(min).all() && at.cmple(max).all()) {
                selection.furniture[index] = true;
                let (a, b) = item.bounds();
                selection.include(a, b);
            }
        }
        Some(selection)
    }

    fn enqueue(&mut self, part: Part, queue: &mut VecDeque<Part>) {
        match part {
            Part::Wall(index) => self.walls[index] = true,
            Part::Room(index) => self.rooms[index] = true,
        }
        queue.push_back(part);
    }

    fn include(&mut self, a: Vec2, b: Vec2) {
        self.min = self.min.min(a).min(b);
        self.max = self.max.max(a).max(b);
    }

    pub fn clamp_delta(&self, delta: Vec2) -> Vec2 {
        // Legacy furniture can straddle the edge. Always keep zero movement valid.
        delta.clamp(
            (-self.min).min(Vec2::ZERO),
            (SHEET - self.max).max(Vec2::ZERO),
        )
    }

    pub fn translate(&mut self, plan: &mut Plan, delta: Vec2) {
        for (wall, chosen) in plan.walls.iter_mut().zip(&self.walls) {
            if *chosen {
                wall.from = (Vec2::from_array(wall.from) + delta).to_array();
                wall.to = (Vec2::from_array(wall.to) + delta).to_array();
            }
        }
        for (room, chosen) in plan.rooms.iter_mut().zip(&self.rooms) {
            if *chosen {
                room.min = (Vec2::from_array(room.min) + delta).to_array();
                room.max = (Vec2::from_array(room.max) + delta).to_array();
            }
        }
        for (item, chosen) in plan.furniture.iter_mut().zip(&self.furniture) {
            if *chosen {
                item.at = (Vec2::from_array(item.at) + delta).to_array();
            }
        }
        self.min += delta;
        self.max += delta;
    }
}

fn walls_meet(a: &Wall, b: &Wall) -> bool {
    let (a0, a1) = a.points();
    let (b0, b1) = b.points();
    if [
        a.projection(b0).1,
        a.projection(b1).1,
        b.projection(a0).1,
        b.projection(a1).1,
    ]
    .into_iter()
    .any(|distance| distance <= 0.06)
    {
        return true;
    }
    let u = a1 - a0;
    let v = b1 - b0;
    let d = u.perp_dot(v);
    if d.abs() < 1e-6 {
        return false;
    }
    let diff = b0 - a0;
    (0.0..=1.0).contains(&(diff.perp_dot(v) / d)) && (0.0..=1.0).contains(&(diff.perp_dot(u) / d))
}

fn wall_touches(wall: &Wall, area: &Area) -> bool {
    let (a, b) = wall.points();
    let min = Vec2::from_array(area.min) - Vec2::splat(0.03);
    let max = Vec2::from_array(area.max) + Vec2::splat(0.03);
    let mut entry = 0.0_f32;
    let mut exit = 1.0_f32;
    for axis in 0..2 {
        let step = b[axis] - a[axis];
        if step.abs() < 1e-6 {
            if a[axis] < min[axis] || a[axis] > max[axis] {
                return false;
            }
        } else {
            let near = (min[axis] - a[axis]) / step;
            let far = (max[axis] - a[axis]) / step;
            entry = entry.max(near.min(far));
            exit = exit.min(near.max(far));
            if entry > exit {
                return false;
            }
        }
    }
    true
}

pub fn pencil_vertices(points: &[Vec2]) -> Vec<Vec2> {
    fn append(points: &[Vec2], output: &mut Vec<Vec2>) {
        let a = points[0];
        let b = points[points.len() - 1];
        let edge = b - a;
        let direction = edge.normalize_or_zero();
        let length = edge.length();
        let farthest = points
            .iter()
            .enumerate()
            .skip(1)
            .take(points.len().saturating_sub(2))
            .map(|(i, p)| {
                let at = (*p - a).dot(direction).clamp(0.0, length);
                (i, p.distance(a + direction * at))
            })
            .max_by(|(_, a), (_, b)| a.total_cmp(b));
        if let Some((index, distance)) = farthest
            && distance > 0.16
        {
            append(&points[..=index], output);
            append(&points[index..], output);
        } else if output.last().is_some_and(|point| point.distance(b) >= 0.24) {
            output.push(b);
        }
    }

    let Some(&first) = points.first() else {
        return Vec::new();
    };
    let mut simplified = vec![first];
    append(points, &mut simplified);
    simplified
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Furniture, FurnitureKind, OpeningKind};

    #[test]
    fn moves_a_connected_house_without_moving_the_neighbor() {
        let mut plan = Plan::default();
        assert!(plan.room(Vec2::new(2., 2.), Vec2::new(7., 7.)));
        assert!(plan.room(Vec2::new(7., 2.), Vec2::new(10., 7.)));
        assert!(plan.room(Vec2::new(14., 2.), Vec2::new(19., 7.)));
        assert!(plan.opening(OpeningKind::Door, Vec2::new(4., 2.)));
        plan.furniture.push(Furniture {
            at: [4., 4.],
            kind: FurnitureKind::Table,
            rotated: false,
        });
        let mut selection = HouseSelection::at(&plan, Vec2::new(4., 4.)).unwrap();
        assert_eq!(selection.rooms, vec![true, true, false]);
        let delta = selection.clamp_delta(Vec2::new(1., 2.));
        selection.translate(&mut plan, delta);
        assert_eq!(plan.rooms[0].min, [3., 4.]);
        assert_eq!(plan.rooms[1].min, [8., 4.]);
        assert_eq!(plan.rooms[2].min, [14., 2.]);
        assert_eq!(plan.furniture[0].at, [5., 6.]);
        assert_eq!(plan.walls[0].openings[0].at, 2.);
        assert!(plan.valid());
    }

    #[test]
    fn pencil_keeps_the_corner_and_removes_straight_samples() {
        let points = [
            Vec2::new(1., 1.),
            Vec2::new(2., 1.02),
            Vec2::new(3., 1.),
            Vec2::new(3.02, 2.),
            Vec2::new(3., 3.),
        ];
        assert_eq!(
            pencil_vertices(&points),
            vec![points[0], points[2], points[4]]
        );
    }
}
