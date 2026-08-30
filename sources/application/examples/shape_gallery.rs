//! A visual gallery of `Path`-based shapes, painted with the real
//! `Path::paint` renderer (concave fill, cubic-Bézier flattening).
//!
//! Run with: `cargo run -p dex --example shape_gallery`

use dex_core::prelude::*;
use dex_nodes::primitives::shapes::{Anchor, Path};
use eframe::egui;

/// Build an anchor at `(x, y)` with optional in/out handles (offsets from pos).
fn anchor(x: f32, y: f32, in_h: Option<(f32, f32)>, out_h: Option<(f32, f32)>) -> Anchor {
    Anchor {
        pos: Vector { x, y },
        in_handle: in_h.map(|(x, y)| Vector { x, y }),
        out_handle: out_h.map(|(x, y)| Vector { x, y }),
    }
}

fn v(x: f32, y: f32) -> Vector {
    Vector { x, y }
}

/// A triangle as a 3-corner closed polygon.
fn triangle() -> Path {
    Path::polygon(
        vec![v(60.0, 0.0), v(120.0, 104.0), v(0.0, 104.0)],
        Color::rgb(80, 140, 240),
        Stroke::new(2.0, Color::rgb(30, 60, 120)),
    )
}

/// A regular pentagon.
fn pentagon() -> Path {
    let r = 58.0;
    let points = (0..5)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
            v(r + r * a.cos(), r + r * a.sin())
        })
        .collect();
    Path::polygon(
        points,
        Color::rgb(120, 200, 140),
        Stroke::new(2.0, Color::rgb(40, 90, 60)),
    )
}

/// A five-point star: a concave polygon (exercises concave fill).
fn star() -> Path {
    let (outer, inner) = (66.0, 27.0);
    let points = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            v(outer + r * a.cos(), outer + r * a.sin())
        })
        .collect();
    Path::polygon(
        points,
        Color::rgb(250, 200, 60),
        Stroke::new(2.0, Color::rgb(150, 110, 10)),
    )
}

/// A right-pointing block arrow.
fn arrow() -> Path {
    Path::polygon(
        vec![
            v(0.0, 30.0),
            v(80.0, 30.0),
            v(80.0, 0.0),
            v(140.0, 55.0),
            v(80.0, 110.0),
            v(80.0, 80.0),
            v(0.0, 80.0),
        ],
        Color::rgb(230, 90, 90),
        Stroke::NONE,
    )
}

/// An open sine wave as a polyline (no fill).
fn wave() -> Path {
    let points = (0..=30)
        .map(|i| {
            let x = i as f32 * 4.5;
            v(x, 55.0 + 35.0 * (x / 22.0).sin())
        })
        .collect();
    Path::polyline(points, Stroke::new(3.0, Color::rgb(70, 120, 220)))
}

/// A rounded blob: a closed path of smooth (mirrored-handle) anchors.
fn blob() -> Path {
    let (cx, cy, r, n) = (62.0, 62.0, 56.0, 6usize);
    let tangent = std::f32::consts::TAU * r / n as f32 * 0.38;
    let anchors = (0..n)
        .map(|i| {
            let a = i as f32 * std::f32::consts::TAU / n as f32;
            Anchor::smooth(
                v(cx + r * a.cos(), cy + r * a.sin()),
                v(-a.sin() * tangent, a.cos() * tangent),
            )
        })
        .collect();
    Path::closed_through(anchors, Color::rgb(180, 120, 230), Stroke::new(2.0, Color::rgb(90, 50, 130)))
}

/// A heart, from four cubic segments with independent in/out handles.
fn heart() -> Path {
    let anchors = vec![
        anchor(60.0, 108.0, Some((50.0, -38.0)), Some((-50.0, -38.0))),
        anchor(20.0, 20.0, Some((-20.0, 20.0)), Some((15.0, -15.0))),
        anchor(60.0, 28.0, Some((-5.0, -18.0)), Some((5.0, -18.0))),
        anchor(100.0, 20.0, Some((-15.0, -15.0)), Some((20.0, 20.0))),
    ];
    Path::closed_through(anchors, Color::rgb(230, 70, 100), Stroke::new(2.0, Color::rgb(150, 30, 60)))
}

/// A rounded rectangle built purely from a `Path`: straight edges joined by
/// quarter-circle Bézier corners. Demonstrates describing `Rect` via `Path`.
fn rounded_rect() -> Path {
    let (w, h, r) = (150.0, 96.0, 22.0);
    let k = r * 0.5523; // cubic approximation of a quarter circle
    let anchors = vec![
        anchor(r, 0.0, Some((-k, 0.0)), None),
        anchor(w - r, 0.0, None, Some((k, 0.0))),
        anchor(w, r, Some((0.0, -k)), None),
        anchor(w, h - r, None, Some((0.0, k))),
        anchor(w - r, h, Some((k, 0.0)), None),
        anchor(r, h, None, Some((-k, 0.0))),
        anchor(0.0, h - r, Some((0.0, k)), None),
        anchor(0.0, r, None, Some((0.0, -k))),
    ];
    Path::closed_through(anchors, Color::rgb(90, 170, 210), Stroke::new(2.0, Color::rgb(40, 90, 120)))
}

struct Gallery {
    shapes: Vec<(&'static str, Path)>,
}

impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let painter = ui.painter().clone();
        let origin = ui.max_rect().min;
        let (cols, cell_w, cell_h) = (3usize, 250.0_f32, 210.0_f32);
        for (i, (name, path)) in self.shapes.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let cell_x = origin.x + col as f32 * cell_w + 30.0;
            let cell_y = origin.y + row as f32 * cell_h + 24.0;
            path.paint(&painter, ScreenPos { x: cell_x, y: cell_y });
            painter.text(
                egui::pos2(cell_x, cell_y + cell_h - 74.0),
                egui::Align2::LEFT_TOP,
                *name,
                egui::FontId::proportional(14.0),
                egui::Color32::from_gray(60),
            );
        }
    }
}

fn main() -> eframe::Result {
    let shapes = vec![
        ("triangle (polygon)", triangle()),
        ("pentagon (polygon)", pentagon()),
        ("star (concave fill)", star()),
        ("arrow (polygon)", arrow()),
        ("wave (open polyline)", wave()),
        ("blob (smooth anchors)", blob()),
        ("heart (bézier handles)", heart()),
        ("rounded rect (path arcs)", rounded_rect()),
    ];
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([780.0, 660.0]),
        ..Default::default()
    };
    eframe::run_native(
        "dex — shape gallery",
        native_options,
        Box::new(|cc| {
            cc.egui_ctx
                .options_mut(|opt| opt.theme_preference = egui::ThemePreference::Light);
            Ok(Box::new(Gallery { shapes }) as Box<dyn eframe::App>)
        }),
    )
}
