//! Rasterise a sheet of gradient-filled polygons, to look at what the fill
//! modes actually come out as.
//!
//! Run with: `cargo run -p dex-nodes --example gradient_sheet -- out.ppm`
//!
//! The triangles are the ones the real painter is handed, shaded the way a GPU
//! shades them — barycentric interpolation of the vertex colours — so what
//! comes out is what the screen shows, minus the antialiasing.

use dex_core::prelude::*;
use dex_nodes::primitives::shapes::{FillMode, Path};

const WIDTH: usize = 780;
const HEIGHT: usize = 300;
const BACKGROUND: [f32; 3] = [246.0, 247.0, 249.0];

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "gradient_sheet.ppm".to_owned());

    let mut pixels = vec![BACKGROUND; WIDTH * HEIGHT];
    for (i, shape) in sheet().into_iter().enumerate() {
        let origin = ScreenPos {
            x: 30.0 + (i % 4) as f32 * 190.0,
            y: 40.0,
        };
        draw(&mut pixels, &shape, origin);
    }

    let mut out = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for pixel in &pixels {
        for channel in pixel {
            out.push(channel.round().clamp(0.0, 255.0) as u8);
        }
    }
    std::fs::write(&path, out).expect("the sheet is written");
    println!("wrote {path}");
}

/// One shape per mode, plus a many-sided one where the radial case matters most.
fn sheet() -> Vec<Path> {
    let deep = Color::rgb(28, 52, 104);
    let bright = Color::rgb(255, 236, 170);
    let square = |mode, angle, from, to| {
        let mut p = Path::polygon(
            vec![
                Vector::new(0.0, 0.0),
                Vector::new(150.0, 0.0),
                Vector::new(150.0, 150.0),
                Vector::new(0.0, 150.0),
            ],
            from,
            Stroke::NONE,
        );
        p.fill_mode = mode;
        p.fill_end = to;
        p.fill_angle = angle;
        p
    };
    let mut star = Path::polygon(star_points(), bright, Stroke::NONE);
    star.fill_mode = FillMode::Radial;
    star.fill_end = Color::rgba(28, 52, 104, 0);

    vec![
        square(FillMode::Solid, 0.0, deep, deep),
        square(FillMode::Linear, 0.0, deep, bright),
        square(FillMode::Radial, 0.0, bright, deep),
        star,
    ]
}

/// A ten-pointed star: concave, so the ear clipping has real work to do.
fn star_points() -> Vec<Vector> {
    (0..20)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / 20.0 - std::f32::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 75.0 } else { 32.0 };
            Vector::new(75.0 + r * angle.cos(), 75.0 + r * angle.sin())
        })
        .collect()
}

/// Shade every triangle of `shape` into `pixels`, over what is already there.
fn draw(pixels: &mut [[f32; 3]], shape: &Path, origin: ScreenPos) {
    let mesh = shape.fill_mesh(origin);
    for triangle in mesh.indices.chunks_exact(3) {
        let corners: Vec<&egui::epaint::Vertex> = triangle
            .iter()
            .map(|i| &mesh.vertices[*i as usize])
            .collect();
        let (a, b, c) = (corners[0].pos, corners[1].pos, corners[2].pos);
        let area = edge(a, b, c);
        if area.abs() < 1e-6 {
            continue;
        }

        let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let max_x = (a.x.max(b.x).max(c.x).ceil() as usize).min(WIDTH - 1);
        let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let max_y = (a.y.max(b.y).max(c.y).ceil() as usize).min(HEIGHT - 1);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
                let (wa, wb, wc) = (
                    edge(b, c, p) / area,
                    edge(c, a, p) / area,
                    edge(a, b, p) / area,
                );
                if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                    continue;
                }
                // egui's vertex colours are premultiplied, so the weighted sum
                // is the colour and the weighted alpha is the coverage.
                let mix = |channel: fn(egui::Color32) -> u8| {
                    wa * channel(corners[0].color) as f32
                        + wb * channel(corners[1].color) as f32
                        + wc * channel(corners[2].color) as f32
                };
                let alpha = mix(|c| c.a()) / 255.0;
                let over = [mix(|c| c.r()), mix(|c| c.g()), mix(|c| c.b())];
                let under = pixels[y * WIDTH + x];
                pixels[y * WIDTH + x] = [
                    over[0] + under[0] * (1.0 - alpha),
                    over[1] + under[1] * (1.0 - alpha),
                    over[2] + under[2] * (1.0 - alpha),
                ];
            }
        }
    }
}

/// Twice the signed area of the triangle `abp`, which is the barycentric weight
/// of `p` against the edge `ab`.
fn edge(a: egui::Pos2, b: egui::Pos2, p: egui::Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
