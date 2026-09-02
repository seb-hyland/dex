//! Small interface glyphs, drawn rather than typeset.

use dex_core::prelude::*;

use crate::primitives::shapes::{Anchor, Path};

/// The shape an [`Icon`] draws.
#[derive(Copy, Default)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub enum Glyph {
    /// Collapse or step back.
    #[default]
    ChevronLeft,
    /// Reveal or step forward.
    ChevronRight,
    /// Fold a panel upwards.
    ChevronUp,
    /// Unfold a panel downwards.
    ChevronDown,
    /// Up a level, out of the current folder.
    ArrowUp,
    /// Run, play, evaluate.
    Play,
    /// Open elsewhere: hand off to an external application.
    External,
    /// Add.
    Plus,
    /// Close, remove, dismiss.
    Cross,
    /// A tick, for a checked box.
    Check,
    /// A directory in a browser.
    Folder,
    /// A file in a browser.
    File,
}

impl Glyph {
    /**
        The glyph as polylines in a unit box, `(0,0)` top-left to `(1,1)`.

        Each entry is `(points, closed)`. Keeping every glyph in the same box
        is what makes them line up when they sit next to each other at
        different sizes.
    */
    fn strokes(self) -> Vec<(Vec<(f32, f32)>, bool)> {
        let open = |pts: Vec<(f32, f32)>| (pts, false);
        match self {
            Self::ChevronLeft => vec![open(vec![(0.64, 0.16), (0.34, 0.5), (0.64, 0.84)])],
            Self::ChevronRight => vec![open(vec![(0.36, 0.16), (0.66, 0.5), (0.36, 0.84)])],
            Self::ChevronUp => vec![open(vec![(0.16, 0.64), (0.5, 0.34), (0.84, 0.64)])],
            Self::ChevronDown => vec![open(vec![(0.16, 0.36), (0.5, 0.66), (0.84, 0.36)])],
            Self::ArrowUp => vec![
                open(vec![(0.5, 0.84), (0.5, 0.18)]),
                open(vec![(0.24, 0.44), (0.5, 0.18), (0.76, 0.44)]),
            ],
            Self::Play => vec![(vec![(0.28, 0.16), (0.84, 0.5), (0.28, 0.84)], true)],
            // A box with its top-right corner opened, and an arrow leaving it.
            Self::External => vec![
                open(vec![
                    (0.56, 0.16),
                    (0.16, 0.16),
                    (0.16, 0.84),
                    (0.84, 0.84),
                    (0.84, 0.46),
                ]),
                open(vec![(0.48, 0.52), (0.86, 0.14)]),
                open(vec![(0.60, 0.14), (0.86, 0.14), (0.86, 0.40)]),
            ],
            Self::Plus => vec![
                open(vec![(0.5, 0.16), (0.5, 0.84)]),
                open(vec![(0.16, 0.5), (0.84, 0.5)]),
            ],
            Self::Cross => vec![
                open(vec![(0.22, 0.22), (0.78, 0.78)]),
                open(vec![(0.78, 0.22), (0.22, 0.78)]),
            ],
            Self::Check => vec![open(vec![(0.18, 0.52), (0.42, 0.76), (0.82, 0.24)])],
            // A tab along the top-left, then the body.
            Self::Folder => vec![(
                vec![
                    (0.08, 0.82),
                    (0.08, 0.20),
                    (0.40, 0.20),
                    (0.48, 0.32),
                    (0.92, 0.32),
                    (0.92, 0.82),
                ],
                true,
            )],
            // A sheet with the corner turned down.
            Self::File => vec![
                (
                    vec![
                        (0.22, 0.14),
                        (0.62, 0.14),
                        (0.80, 0.34),
                        (0.80, 0.86),
                        (0.22, 0.86),
                    ],
                    true,
                ),
                open(vec![(0.62, 0.14), (0.62, 0.34), (0.80, 0.34)]),
            ],
        }
    }

    /// Whether the glyph is a solid shape rather than line art.
    fn solid(self) -> bool {
        matches!(self, Self::Play)
    }
}

/**
    A [`Glyph`] painted into a square of `size`, in `color`.

    ```python
    ctx.draw_node(dex.Icon.new(dex.Glyph.Plus, 12.0, dex.Theme.ink_muted()), constraints)
    ```
*/
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
pub struct Icon {
    pub glyph: Glyph,
    /// The side of the square the glyph is drawn into.
    pub size: f32,
    pub color: Color,
    /// Line weight. Left at zero, it scales with `size`.
    pub width: f32,
}

#[utils::dynamic_methods]
impl Icon {
    /// A glyph at `size`, in `color`, with a weight that suits the size.
    pub fn new(glyph: Glyph, size: f32, color: Color) -> Self {
        Self {
            glyph,
            size,
            color,
            width: 0.0,
        }
    }

    /// The same, with the line weight chosen by hand.
    pub fn weighted(glyph: Glyph, size: f32, color: Color, width: f32) -> Self {
        Self {
            glyph,
            size,
            color,
            width,
        }
    }
}

impl Icon {
    /// The stroke to draw with: the given weight, else one scaled to `size`.
    fn stroke(&self) -> Stroke {
        let width = if self.width > 0.0 {
            self.width
        } else {
            // Thin enough to stay crisp, never so thin it disappears.
            (self.size * 0.115).clamp(1.0, 2.0)
        };
        Stroke::new(width, self.color)
    }

    /// Paint the glyph with its box's top-left at `origin`.
    pub fn paint(&self, painter: &egui::Painter, origin: ScreenPos) {
        let stroke = self.stroke();
        let solid = self.glyph.solid();
        for (points, closed) in self.glyph.strokes() {
            let scaled: Vec<Vector> = points
                .into_iter()
                .map(|(x, y)| Vector {
                    x: x * self.size,
                    y: y * self.size,
                })
                .collect();
            // A solid glyph is its own shape; line art is an outline of one.
            let path = if solid {
                // Stroked as well as filled, so a small triangle keeps the
                // visual weight of the strokes it sits beside.
                Path::polygon(scaled, self.color, stroke)
            } else {
                Path::unfilled(
                    scaled.into_iter().map(Anchor::corner).collect(),
                    closed,
                    stroke,
                )
            };
            path.paint(painter, origin);
        }
    }
}

#[utils::dynamic_node]
impl Node for Icon {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "An Icon".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let origin = ctx.constraints.pos;
        self.paint(ctx.ui.painter(), origin);
        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector::splat(self.size),
            )),
        }
    }
}

defhandlers! { Icon {
    actions: [
        SetIconGlyph { glyph: Glyph } => (this, a) { this.glyph = a.glyph; },
        SetIconColor { color: Color } => (this, a) { this.color = a.color; },
    ],
}}
