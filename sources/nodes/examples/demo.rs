//! End-to-end demo of the workspace framework.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p nodes --example demo
//! ```
//!
//! Layout: a static [`HorizontalLayout`] root holds a `BarChart` (left) and a
//! `Scatter` (right). The scatter is drawn *last*, so it draws the connector
//! lines on top of both panels.
//!
//! Scatter internals (all framework nodes):
//!   - a [`CanvasLayout`] built with `Default` + `push_child`, holding one
//!     `Circle` per data row (all the same colour).
//!   - a full-panel [`InteractionBox`] (a shared-uid workspace node) that the
//!     top-level `Scatter` queries with `WasHovered` each frame.
//!
//! On hover, the scatter asks the bar chart for its rect (built-in `Region`
//! request) and draws light `Line` connectors from each dot to its category bar.
//!
//! Requires the `InteractionBox::to_sense` fix (see the message accompanying
//! this file) — otherwise `WasHovered` is always false and no lines appear.
//! Note also that `InteractionBox` is drawn directly (sized) rather than pushed
//! onto the canvas, because `CanvasLayout` gives its children no size.

use dex_core::prelude::*;
use dex_nodes::{
    layouts::{
        canvas::{CanvasLayout, CanvasNode},
        horizontal::HorizontalLayout,
    },
    primitives::{
        interaction::{InteractionBox, WasHovered},
        shapes::{Circle, Line, Rect},
        text::Label,
    },
};
use eframe::egui;
use egui::{Color32, FontId, Pos2, Stroke};
use utils::Reset;

// ---------------------------------------------------------------------------
// Hardcoded data: (numeric1, numeric2, categorical)
// ---------------------------------------------------------------------------

const DATA: &[(f32, f32, &str)] = &[
    (1.5, 8.0, "A"),
    (2.5, 6.5, "A"),
    (1.0, 7.0, "A"),
    (3.0, 7.5, "A"),
    (5.0, 4.0, "B"),
    (6.0, 5.5, "B"),
    (4.5, 3.0, "B"),
    (8.5, 2.0, "C"),
    (9.0, 3.5, "C"),
];

const CATEGORIES: &[&str] = &["A", "B", "C"];

const X_RANGE: (f32, f32) = (0.0, 10.0);
const Y_RANGE: (f32, f32) = (0.0, 10.0);

/// Inner margin inside each panel.
const PAD: f32 = 44.0;

const TEXT: Color32 = Color32::from_gray(220);
const PANEL_BG: Color32 = Color32::from_gray(38);
const PANEL_BORDER: Color32 = Color32::from_gray(80);
/// Light, always-the-same scatter dot colour.
const DOT_COLOR: Color32 = Color32::from_rgb(130, 180, 245);
/// Light connector colour.
const LINE_COLOR: Color32 = Color32::from_rgb(235, 235, 240);
const DOT_RADIUS: f32 = 6.0;

// Fixed scatter plot box (canvas positions are fixed, not responsive).
const PLOT_MARGIN: f32 = 52.0;
const PLOT_W: f32 = 320.0;
const PLOT_H: f32 = 300.0;

fn category_color(cat: &str) -> Color32 {
    match cat {
        "A" => Color32::from_rgb(224, 90, 90),
        "B" => Color32::from_rgb(90, 190, 110),
        "C" => Color32::from_rgb(90, 140, 224),
        _ => Color32::GRAY,
    }
}

// ---------------------------------------------------------------------------
// Shared geometry
// ---------------------------------------------------------------------------

/// A data row's dot position *within* the scatter canvas (canvas-relative).
fn dot_canvas_pos(n1: f32, n2: f32) -> Pos2 {
    let nx = (n1 - X_RANGE.0) / (X_RANGE.1 - X_RANGE.0);
    let ny = (n2 - Y_RANGE.0) / (Y_RANGE.1 - Y_RANGE.0);
    Pos2 {
        x: PLOT_MARGIN + nx * PLOT_W,
        y: PLOT_MARGIN + (1.0 - ny) * PLOT_H, // invert: higher n2 -> higher up
    }
}

/// Per-category bar geometry inside a bar-chart panel `rect`:
/// (category, count, bar top-left, bar size, top-centre anchor).
fn category_bars(rect: ScreenRegion) -> Vec<(String, u32, ScreenPos, Vector, ScreenPos)> {
    let counts: Vec<(String, u32)> = CATEGORIES
        .iter()
        .map(|c| {
            let n = DATA.iter().filter(|(_, _, cc)| cc == c).count() as u32;
            ((*c).to_owned(), n)
        })
        .collect();
    let max = counts.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1) as f32;

    let size = rect.size();
    let plot_w = size.x - 2.0 * PAD;
    let plot_h = size.y - 2.0 * PAD;
    let baseline_y = rect.min.y + size.y - PAD;
    let slot = plot_w / counts.len() as f32;
    let bar_w = (slot * 0.55).min(64.0);

    counts
        .iter()
        .enumerate()
        .map(|(i, (cat, count))| {
            let height = (*count as f32 / max) * plot_h;
            let center_x = rect.min.x + PAD + slot * (i as f32 + 0.5);
            let top_y = baseline_y - height;
            (
                cat.clone(),
                *count,
                ScreenPos {
                    x: center_x - bar_w / 2.0,
                    y: top_y,
                },
                Vector {
                    x: bar_w,
                    y: height,
                },
                ScreenPos {
                    x: center_x,
                    y: top_y,
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Small helpers — everything renders through framework primitive nodes.
// ---------------------------------------------------------------------------

fn place<N: Node>(ctx: &mut DrawContext, node: &N, key: u64, pos: PositionConstraint) {
    let id = NodeUid::new_local(ctx.id, key);
    let _ = ctx.draw_node(
        node,
        id,
        DrawConstraints {
            pos,
            x: None,
            y: None,
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        },
    );
}

/// Draw a sized workspace node (canvas / interaction box) filling `rect`.
fn draw_filling(ctx: &mut DrawContext, id: NodeUid, rect: ScreenRegion) {
    let size = rect.size();
    let _ = ctx.draw_workspace_node(
        id,
        DrawConstraints {
            pos: PositionConstraint::TopLeft(rect.min),
            x: Some(AxisConstraint::Exactly(size.x)),
            y: Some(AxisConstraint::Exactly(size.y)),
            wrap: WrapConstraints::NotAllowed,
            should_clip: true,
        },
    );
}

fn label(text: &str, size: f32) -> Label {
    Label {
        text: text.to_owned(),
        singleline: true,
        font: FontId::proportional(size),
        color: TEXT,
    }
}

fn background(size: Vector) -> Rect {
    Rect {
        size,
        corner_radius: 6.0,
        fill_color: PANEL_BG,
        border: Stroke::new(1.0, PANEL_BORDER),
    }
}

/// This node's top-left origin and size. `HorizontalLayout` is greedy: the
/// first child claims `width_frac` (0.5) of its offered width, the second fills
/// the rest (1.0). Height fills fully.
fn box_geometry(ctx: &DrawContext, width_frac: f32, fallback: Vector) -> (ScreenPos, Vector) {
    let avail_x = ctx
        .constraints
        .x
        .map(|a| a.provided_value())
        .filter(|v| v.is_finite())
        .unwrap_or(fallback.x);
    let avail_y = ctx
        .constraints
        .y
        .map(|a| a.provided_value())
        .filter(|v| v.is_finite())
        .unwrap_or(fallback.y);
    let size = Vector {
        x: avail_x * width_frac,
        y: avail_y,
    };
    (ctx.constraints.pos.to_top_left(size), size)
}

// ---------------------------------------------------------------------------
// BarChart (left panel)
// ---------------------------------------------------------------------------

const BAR_FALLBACK: Vector = Vector { x: 340.0, y: 380.0 };

#[derive(Clone, Reset, serde::Serialize, serde::Deserialize)]
struct BarChart;

#[typetag::serde]
impl Node for BarChart {
    fn type_name(&self) -> String {
        "BarChart".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        // Left panel: claim half the offered width so the scatter gets the rest.
        let (origin, size) = box_geometry(&ctx, 0.5, BAR_FALLBACK);
        let rect = ScreenRegion::from_min_size(origin, size);

        place(
            &mut ctx,
            &background(size),
            0,
            PositionConstraint::TopLeft(origin),
        );
        place(
            &mut ctx,
            &label("BarChart: rows per category", 15.0),
            1,
            PositionConstraint::TopLeft(origin + Vector { x: 14.0, y: 12.0 }),
        );

        for (i, (cat, count, bar_origin, bar_size, anchor)) in
            category_bars(rect).into_iter().enumerate()
        {
            place(
                &mut ctx,
                &Rect {
                    size: bar_size,
                    corner_radius: 3.0,
                    fill_color: category_color(&cat),
                    border: Stroke::NONE,
                },
                10 + i as u64,
                PositionConstraint::TopLeft(bar_origin),
            );
            place(
                &mut ctx,
                &label(&count.to_string(), 14.0),
                100 + i as u64,
                PositionConstraint::TopLeft(ScreenPos {
                    x: anchor.x - 4.0,
                    y: anchor.y - 20.0,
                }),
            );
            place(
                &mut ctx,
                &label(&cat, 14.0),
                200 + i as u64,
                PositionConstraint::TopLeft(ScreenPos {
                    x: anchor.x - 5.0,
                    y: rect.min.y + size.y - PAD + 8.0,
                }),
            );
        }

        DrawResult::Complete { region: Some(rect) }
    }
}

defhandlers! { BarChart {} }

// ---------------------------------------------------------------------------
// Scatter (right panel) — canvas of dots + a queried InteractionBox
// ---------------------------------------------------------------------------

const SCATTER_FALLBACK: Vector = Vector { x: 400.0, y: 380.0 };

#[derive(Clone, Reset, serde::Serialize, serde::Deserialize)]
struct Scatter {
    /// CanvasLayout holding the dot `Circle`s.
    canvas: NodeUid,
    /// Full-panel InteractionBox; queried each frame for hover.
    interaction: NodeUid,
    /// Sibling bar chart, queried for its rect to anchor the connector lines.
    barchart: NodeUid,
}

#[typetag::serde]
impl Node for Scatter {
    fn type_name(&self) -> String {
        "Scatter".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        // Right panel: fills the width the layout leaves after the bar chart.
        let (origin, size) = box_geometry(&ctx, 1.0, SCATTER_FALLBACK);
        let rect = ScreenRegion::from_min_size(origin, size);

        place(
            &mut ctx,
            &background(size),
            0,
            PositionConstraint::TopLeft(origin),
        );
        // Dots, via the CanvasLayout (positions were set with push_child).
        draw_filling(&mut ctx, self.canvas, rect);
        place(
            &mut ctx,
            &label("Scatter: numeric1 vs numeric2  (hover me)", 15.0),
            1,
            PositionConstraint::TopLeft(origin + Vector { x: 14.0, y: 12.0 }),
        );

        // Full-panel interaction sensor, then query its hover state this frame.
        draw_filling(&mut ctx, self.interaction, rect);
        let hovered = ctx
            .workspace
            .send_request(self.interaction, WasHovered)
            .unwrap_or(false);

        if hovered
            && let Some(bar_rect) = ctx.workspace.send_request(self.barchart, Region).flatten()
        {
            let anchors = category_bars(bar_rect);
            for (i, (n1, n2, cat)) in DATA.iter().enumerate() {
                let Some((.., anchor)) = anchors.iter().find(|(c, ..)| c == cat) else {
                    continue;
                };
                // Dot centre = panel origin + its canvas position (+ half the
                // radius, since Circle centres on its box).
                let cp = dot_canvas_pos(*n1, *n2);
                let start = ScreenPos {
                    x: origin.x + cp.x + DOT_RADIUS / 2.0,
                    y: origin.y + cp.y + DOT_RADIUS / 2.0,
                };
                let span = Vector {
                    x: anchor.x - start.x,
                    y: anchor.y - start.y,
                };
                place(
                    &mut ctx,
                    &Line {
                        span,
                        stroke: Stroke::new(1.5, LINE_COLOR),
                    },
                    900 + i as u64,
                    PositionConstraint::TopLeft(start),
                );
            }
        }

        DrawResult::Complete { region: Some(rect) }
    }
}

defhandlers! { Scatter {} }

// ---------------------------------------------------------------------------
// App wiring
// ---------------------------------------------------------------------------

fn build_workspace() -> Workspace {
    let mut ws = Workspace::new_with_root(Box::new(HorizontalLayout {
        children: vec![],
        allow_wrap: false,
        wrap_spacing: 0.0,
    }));

    let bar_uid = ws.insert_node(Box::new(BarChart));

    // Scatter canvas: one Circle per row, positioned via push_child.
    let mut canvas = CanvasLayout::default();
    for (n1, n2, _cat) in DATA {
        let dot_uid = ws.insert_node(Box::new(Circle {
            radius: DOT_RADIUS,
            fill_color: DOT_COLOR,
            border: Stroke::new(1.0, Color32::from_gray(20)),
        }));
        canvas.push_child(CanvasNode {
            canvas_pos: dot_canvas_pos(*n1, *n2),
            id: dot_uid,
        });
    }
    let canvas_uid = ws.insert_node(Box::new(canvas));

    // Full-panel hover sensor, shared by uid with the Scatter node.
    let interaction_uid = ws.insert_node(Box::new({
        let mut ib = InteractionBox::default();
        ib.senses_hover = true;
        ib
    }));

    let scatter_uid = ws.insert_node(Box::new(Scatter {
        canvas: canvas_uid,
        interaction: interaction_uid,
        barchart: bar_uid,
    }));

    // Static root: bar chart on the left, scatter on the right (drawn last, so
    // its hover connector lines land on top).
    let root_uid = ws.insert_node(Box::new(HorizontalLayout {
        children: vec![bar_uid.into(), scatter_uid.into()],
        allow_wrap: false,
        wrap_spacing: 0.0,
    }));
    ws.set_root(root_uid);

    ws
}

struct DemoApp {
    workspace: Workspace,
}

impl eframe::App for DemoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().request_repaint();
        let area = ui.max_rect();
        self.workspace.draw_frame(ui, area);
    }
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "dex — scatter + bar chart demo",
        native_options,
        Box::new(|_cc| {
            Ok(Box::new(DemoApp {
                workspace: build_workspace(),
            }) as Box<dyn eframe::App>)
        }),
    )
}
