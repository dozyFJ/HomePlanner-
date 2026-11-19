use std::{fs, io, path::Path, time::SystemTime};

use egui::{Color32, ColorImage, TextureHandle, TextureOptions};
use glam::Vec2;

use crate::plan::{Plan, SHEET};

pub struct Photo {
    pub modified: SystemTime,
    pub texture: TextureHandle,
    width: usize,
    height: usize,
    gray: Vec<u8>,
}

impl Photo {
    pub fn read(path: &Path, modified: SystemTime, ctx: &egui::Context) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Некорректное фото плана");
        let width = bytes
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_be_bytes)
            .ok_or_else(invalid)? as usize;
        let height = bytes
            .get(4..8)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_be_bytes)
            .ok_or_else(invalid)? as usize;
        if !(1..=1024).contains(&width)
            || !(1..=1024).contains(&height)
            || bytes.len() != 8 + width * height
        {
            return Err(invalid());
        }
        let gray = bytes[8..].to_vec();
        let pixels = gray
            .iter()
            .map(|&luma| Color32::from_white_alpha((255 - luma) / 2))
            .collect::<Vec<_>>();
        let pixels = pixels
            .into_iter()
            .map(|alpha| Color32::from_rgba_unmultiplied(14, 34, 45, alpha.a()))
            .collect();
        let texture = ctx.load_texture(
            "plan-photo",
            ColorImage {
                size: [width, height],
                pixels,
            },
            TextureOptions::LINEAR,
        );
        Ok(Self {
            modified,
            texture,
            width,
            height,
            gray,
        })
    }

    pub fn mapped_point(&self, [x, y]: [f32; 2], portrait: bool) -> Vec2 {
        let sheet = if portrait {
            Vec2::new(SHEET.y, SHEET.x)
        } else {
            SHEET
        };
        let pixels = Vec2::new(self.width as f32, self.height as f32);
        let scale = (sheet / pixels).min_element() * 0.96;
        let q = (sheet - pixels * scale) * 0.5 + Vec2::new(x, y) * pixels * scale;
        if portrait {
            Vec2::new(q.y, SHEET.y - q.x)
        } else {
            q
        }
    }

    pub fn insert_walls(&self, plan: &mut Plan, portrait: bool) -> usize {
        let mut count = 0;
        for [a, b] in segments(&self.gray, self.width, self.height)
            .into_iter()
            .take(120)
        {
            if plan.wall(
                self.mapped_point(a, portrait),
                self.mapped_point(b, portrait),
            ) {
                count += 1;
            }
        }
        count
    }
}

#[derive(Clone, Copy)]
struct Run {
    row: usize,
    start: usize,
    end: usize,
}

fn segments(gray: &[u8], width: usize, height: usize) -> Vec<[[f32; 2]; 2]> {
    let Some(threshold) = otsu(gray) else {
        return Vec::new();
    };
    if gray.iter().filter(|&&pixel| pixel <= threshold).count() * 2 > gray.len() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for vertical in [false, true] {
        let (rows, columns) = if vertical {
            (width, height)
        } else {
            (height, width)
        };
        let mut candidates: Vec<Run> = Vec::new();
        let min_length = (columns * 8 / 100).max(14);
        let max_gap = (columns / 260).max(2);
        for row in 0..rows {
            let mut start = None;
            let mut last = 0;
            for col in 0..=columns {
                let ink = col < columns && {
                    let index = if vertical {
                        col * width + row
                    } else {
                        row * width + col
                    };
                    gray[index] <= threshold
                };
                if ink {
                    if start.is_none() {
                        start = Some(col);
                    }
                    last = col;
                } else if col == columns || col.saturating_sub(last) > max_gap {
                    if let Some(first) = start.take() {
                        if last - first + 1 >= min_length {
                            candidates.push(Run {
                                row,
                                start: first,
                                end: last,
                            });
                        }
                    }
                }
            }
        }
        candidates.sort_unstable_by_key(|line| std::cmp::Reverse(line.end - line.start));
        let mut kept: Vec<Run> = Vec::new();
        let thickness = (rows / 90).max(3);
        for line in candidates {
            if kept.iter().any(|prior| {
                let overlap = prior
                    .end
                    .min(line.end)
                    .saturating_sub(prior.start.max(line.start));
                prior.row.abs_diff(line.row) <= thickness
                    && overlap * 5 >= (line.end - line.start) * 3
            }) {
                continue;
            }
            let p = |col: usize| {
                let across = col as f32 / columns as f32;
                let along = line.row as f32 / rows as f32;
                if vertical {
                    [along, across]
                } else {
                    [across, along]
                }
            };
            lines.push([p(line.start), p(line.end)]);
            kept.push(line);
            if kept.len() == 60 {
                break;
            }
        }
    }
    lines
}

fn otsu(gray: &[u8]) -> Option<u8> {
    let mut histogram = [0_u64; 256];
    for &value in gray {
        histogram[value as usize] += 1;
    }
    let total = gray.len() as f64;
    let sum = histogram
        .iter()
        .enumerate()
        .map(|(i, count)| i as f64 * *count as f64)
        .sum::<f64>();
    let (mut count, mut left_sum, mut best, mut cutoff) = (0.0, 0.0, 0.0, 145_u8);
    for (value, &bin) in histogram.iter().enumerate() {
        count += bin as f64;
        left_sum += value as f64 * bin as f64;
        if count == 0.0 || count >= total {
            continue;
        }
        let difference = left_sum / count - (sum - left_sum) / (total - count);
        let score = count * (total - count) * difference * difference;
        if score > best {
            best = score;
            cutoff = value as u8;
        }
    }
    (best >= 1.0).then_some(cutoff.clamp(25, 225))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_horizontal_and_vertical_wall_strokes() {
        let mut pixels = vec![255; 100 * 80];
        for x in 10..80 {
            pixels[20 * 100 + x] = 10;
        }
        for y in 20..70 {
            pixels[y * 100 + 60] = 10;
        }
        assert_eq!(segments(&pixels, 100, 80).len(), 2);
        assert!(segments(&vec![255; 100 * 80], 100, 80).is_empty());
        assert!(segments(&vec![0; 100 * 80], 100, 80).is_empty());
    }
}
