use egui::{
    Color32, ColorImage, Mesh, Painter, Pos2, TextureHandle, TextureOptions, epaint::Vertex,
};
use glam::{Vec2, Vec3};
use wolfcraft_assets::{AtlasKind, Image, TextureAtlas, UvRect};

use crate::plan::FurnitureKind;

#[derive(Clone, Copy)]
pub enum Material {
    Oak,
    Walnut,
    Linen,
    Teal,
    Upholstery,
    Metal,
    Shade,
}

impl Material {
    const ALL: [Self; 7] = [
        Self::Oak,
        Self::Walnut,
        Self::Linen,
        Self::Teal,
        Self::Upholstery,
        Self::Metal,
        Self::Shade,
    ];

    fn texture(self) -> Image {
        let mut image = Image::new(96, 96);
        let (base, grain) = match self {
            Self::Oak => ([182, 135, 87], 20.0),
            Self::Walnut => ([115, 75, 48], 17.0),
            Self::Linen => ([222, 213, 190], 8.0),
            Self::Teal => ([70, 132, 129], 10.0),
            Self::Upholstery => ([141, 155, 159], 9.0),
            Self::Metal => ([71, 77, 79], 3.0),
            Self::Shade => ([228, 201, 150], 6.0),
        };
        for y in 0..image.height {
            for x in 0..image.width {
                let xf = x as f32;
                let yf = y as f32;
                let noise = (x.wrapping_mul(747796405) ^ y.wrapping_mul(2891336453))
                    .wrapping_mul(277803737);
                let fleck = ((noise >> 24) as f32 / 255.0 - 0.5) * 5.0;
                let variation = match self {
                    Self::Oak | Self::Walnut => {
                        (yf * 0.50 + (xf * 0.056).sin() * 0.8).sin() * 0.7
                            + (yf * 1.24 + (xf * 0.09 + yf * 0.12).sin()).sin() * 0.3
                    }
                    Self::Metal => (yf * 1.4).sin(),
                    _ => ((x % 3) as f32 - 1.0) * 0.38 + ((y % 3) as f32 - 1.0) * 0.38,
                };
                let channel =
                    |value: u8| (value as f32 + variation * grain + fleck).clamp(0.0, 255.0) as u8;
                image.set(
                    x,
                    y,
                    [channel(base[0]), channel(base[1]), channel(base[2]), 255],
                );
            }
        }
        image
    }
}

pub struct Part {
    pub position: Vec3,
    pub size: Vec3,
    pub material: Material,
}

impl FurnitureKind {
    pub const ALL: [Self; 5] = [
        Self::Bed,
        Self::Table,
        Self::Sofa,
        Self::Cabinet,
        Self::Lamp,
    ];

    pub fn dimensions(self) -> Vec3 {
        match self {
            Self::Bed => Vec3::new(1.6, 0.90, 2.0),
            Self::Table => Vec3::new(1.4, 0.75, 0.85),
            Self::Sofa => Vec3::new(2.1, 0.83, 0.85),
            Self::Cabinet => Vec3::new(1.16, 1.85, 0.58),
            Self::Lamp => Vec3::new(0.44, 1.57, 0.44),
        }
    }

    pub fn parts(self) -> Vec<Part> {
        use Material::*;
        let mut parts = Vec::new();
        let mut add = |(x, bottom, z), (w, h, d), material| {
            parts.push(Part {
                position: Vec3::new(x, bottom + h * 0.5, z),
                size: Vec3::new(w, h, d),
                material,
            });
        };
        match self {
            Self::Bed => {
                add((0., 0.12, 0.), (1.60, 0.18, 2.00), Oak);
                add((0., 0.12, -0.94), (1.60, 0.78, 0.12), Oak);
                add((0., 0.30, 0.04), (1.52, 0.16, 1.84), Linen);
                add((0., 0.46, 0.30), (1.53, 0.055, 1.28), Teal);
                for x in [-0.38, 0.38] {
                    add((x, 0.46, -0.63), (0.64, 0.10, 0.40), Linen);
                }
                for x in [-0.70, 0.70] {
                    for z in [-0.87, 0.87] {
                        add((x, 0., z), (0.10, 0.12, 0.10), Walnut);
                    }
                }
            }
            Self::Table => {
                add((0., 0.69, 0.), (1.4, 0.06, 0.85), Oak);
                add((0., 0.56, 0.), (1.21, 0.13, 0.65), Walnut);
                for x in [-0.54, 0.54] {
                    for z in [-0.28, 0.28] {
                        add((x, 0., z), (0.075, 0.66, 0.075), Walnut);
                    }
                }
            }
            Self::Sofa => {
                add((0., 0.10, 0.), (2.1, 0.27, 0.85), Upholstery);
                add((0., 0.37, -0.33), (2.04, 0.46, 0.19), Upholstery);
                for x in [-0.93, 0.93] {
                    add((x, 0.37, 0.), (0.22, 0.24, 0.85), Upholstery);
                    for z in [-0.31, 0.31] {
                        add((x, 0., z), (0.10, 0.10, 0.10), Walnut);
                    }
                }
                for x in [-0.43, 0.43] {
                    add((x, 0.36, 0.10), (0.80, 0.12, 0.63), Teal);
                    add((x, 0.48, -0.20), (0.80, 0.28, 0.15), Teal);
                }
            }
            Self::Cabinet => {
                add((0., 0., -0.02), (1.16, 1.85, 0.54), Walnut);
                for x in [-0.285, 0.285] {
                    add((x, 0.07, 0.263), (0.55, 1.73, 0.026), Oak);
                }
                for x in [-0.08, 0.08] {
                    add((x, 0.83, 0.284), (0.022, 0.22, 0.012), Metal);
                }
            }
            Self::Lamp => {
                add((0., 0., 0.), (0.36, 0.034, 0.36), Metal);
                add((0., 0.035, 0.), (0.035, 1.31, 0.035), Metal);
                add((0., 1.30, 0.), (0.44, 0.13, 0.44), Shade);
                add((0., 1.43, 0.), (0.37, 0.09, 0.37), Shade);
                add((0., 1.52, 0.), (0.31, 0.05, 0.31), Linen);
            }
        }
        parts
    }
}

pub struct FurnishingAtlas {
    texture: TextureHandle,
    materials: [UvRect; Material::ALL.len()],
    plans: [UvRect; FurnitureKind::ALL.len()],
}

impl FurnishingAtlas {
    pub fn build() -> Result<TextureAtlas, String> {
        let materials = Material::ALL.map(Material::texture);
        let tops = FurnitureKind::ALL.map(|kind| plan_sprite(kind, &materials));
        let sprites = materials
            .into_iter()
            .enumerate()
            .map(|(i, image)| (format!("material_{i}"), image))
            .chain(
                tops.into_iter()
                    .enumerate()
                    .map(|(i, image)| (format!("plan_{i}"), image)),
            )
            .collect();
        TextureAtlas::build(AtlasKind::Entities, sprites)
    }

    pub fn upload(ctx: &egui::Context, atlas: &TextureAtlas) -> Self {
        let image = atlas.image();
        Self {
            texture: ctx.load_texture(
                "furnishing-atlas",
                ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.pixels,
                ),
                TextureOptions::LINEAR,
            ),
            materials: std::array::from_fn(|index| atlas.uv(index)),
            plans: std::array::from_fn(|index| atlas.uv(Material::ALL.len() + index)),
        }
    }

    pub fn material(&self, material: Material) -> UvRect {
        self.materials[material as usize]
    }

    pub fn paint(&self, painter: &Painter, kind: FurnitureKind, corners: [Pos2; 4]) {
        let uv = uv_corners(self.plans[kind as usize]);
        let mut mesh = Mesh::with_texture(self.texture.id());
        for (pos, [u, v]) in corners.into_iter().zip(uv) {
            mesh.vertices.push(Vertex {
                pos,
                uv: egui::pos2(u, v),
                color: Color32::WHITE,
            });
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
    }
}

pub fn uv_corners(rect: UvRect) -> [[f32; 2]; 4] {
    let [u0, v0] = rect.min;
    let [u1, v1] = rect.max;
    [[u0, v0], [u1, v0], [u1, v1], [u0, v1]]
}

fn plan_sprite(kind: FurnitureKind, materials: &[Image; Material::ALL.len()]) -> Image {
    let size = kind.dimensions();
    let width = (size.x * 60.0).ceil() as u32;
    let height = (size.z * 60.0).ceil() as u32;
    let mut image = Image::new(width, height);
    let mut parts = kind.parts();
    parts.sort_by(|a, b| {
        (a.position.y + a.size.y * 0.5).total_cmp(&(b.position.y + b.size.y * 0.5))
    });
    for part in parts {
        let center = Vec2::new(part.position.x, part.position.z);
        let half = Vec2::new(part.size.x, part.size.z) * 0.5;
        let material = &materials[part.material as usize];
        for y in 0..height {
            for x in 0..width {
                let point = Vec2::new(
                    (x as f32 + 0.5) / width as f32 - 0.5,
                    (y as f32 + 0.5) / height as f32 - 0.5,
                ) * Vec2::new(size.x, size.z);
                let local = point - center;
                if local.abs().cmple(half).all() {
                    let uv = (local / half + Vec2::ONE) * 0.5;
                    let mut color = material.get(
                        (uv.x * (material.width - 1) as f32) as u32,
                        (uv.y * (material.height - 1) as f32) as u32,
                    );
                    if (half - local.abs()).min_element() < 0.015 {
                        color[..3]
                            .iter_mut()
                            .for_each(|c| *c = (*c as f32 * 0.67) as u8);
                    }
                    image.set(x, y, color);
                }
            }
        }
    }
    image
}
