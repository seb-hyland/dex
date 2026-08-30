use dex_core::prelude::*;
use egui::{
    Color32, Mesh, Painter, Pos2, Sense, Shape, ecolor::HsvaGamma, lerp, pos2, text::LayoutJob,
};
use serde::{Deserialize, Serialize};
use utils::Transient;

/// The swatch at the right of the collapsed row.
const SWATCH_W: f32 = 36.0;
/// Space between the stacked parts of the open picker.
const GAP: f32 = 4.0;
/// The saturation/value square's height, as a fraction of the width.
const SQUARE_ASPECT: f32 = 0.62;
const BAR_H: f32 = 12.0;
/// Used only when the picker is offered no width at all.
const FALLBACK_WIDTH: f32 = 200.0;
/// A picker fills a narrow menu, but past this it only gets harder to aim.
const MAX_WIDTH: f32 = 240.0;

/// A colour swatch that opens a picker beneath it.
#[utils::dynamic_type]
#[utils::portable]
pub struct ColorPicker {
    pub color: Color,
    /// Named to the left of the swatch.
    pub label: String,
    /// Whether the picking area is open beneath the swatch row.
    pub expanded: bool,
    /// Offer an alpha bar as well as hue and saturation/value.
    pub alpha: bool,

    pub font: Font,
    pub text_color: Color,

    /// The colour under the pointer, until it is committed.
    live: Transient<Color>,
    /// Hue and saturation do not survive a round trip through black or grey,
    /// so the coordinates that produced the current colour are kept.
    coords: Transient<CoordCache>,
}

/// The picker coordinates `srgba` was built from.
#[derive(Clone, Serialize, Deserialize)]
struct CoordCache {
    srgba: [u8; 4],
    hsva: [f32; 4],
}

#[utils::dynamic_methods]
impl ColorPicker {
    pub fn new(label: String, color: Color) -> Self {
        Self {
            color,
            label,
            expanded: false,
            alpha: true,
            font: Font::proportional(13.0),
            text_color: Color::BLACK,
            live: Transient::default(),
            coords: Transient::default(),
        }
    }

    /// Build a picker into `ws` and return its id.
    pub fn build(ws: WorkspaceActionHandle, label: String, color: Color) -> NodeUid<ColorPicker> {
        ws.insert_node(Self::new(label, color))
    }

    /// The colour on show: the one being dragged out, else the committed one.
    #[dynamic(skip)]
    pub fn shown(&self) -> Color {
        self.live.val().unwrap_or(self.color)
    }

    /// The picker coordinates for `color`, preferring the ones that produced it.
    #[dynamic(skip)]
    fn coords_for(&self, color: Color) -> HsvaGamma {
        let srgba = [color.r, color.g, color.b, color.a];
        if let Some(cached) = self.coords.val().as_ref()
            && cached.srgba == srgba
        {
            let [h, s, v, a] = cached.hsva;
            return HsvaGamma { h, s, v, a };
        }
        HsvaGamma::from(Color32::from_rgba_unmultiplied(
            color.r, color.g, color.b, color.a,
        ))
    }

    /// Take `coords` as the new colour, remembering where they came from.
    #[dynamic(skip)]
    fn move_to(&self, coords: HsvaGamma) -> Color {
        let color: Color = Color32::from(coords).into();
        self.coords.set(CoordCache {
            srgba: [color.r, color.g, color.b, color.a],
            hsva: [coords.h, coords.s, coords.v, coords.a],
        });
        self.live.set(color);
        color
    }
}

#[utils::dynamic_node]
impl Node for ColorPicker {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Colour Picker".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let mut job = LayoutJob::single_section(
            self.label.clone(),
            self.font.text_format(ctx.ui.ctx(), self.text_color),
        );
        job.break_on_newline = false;
        let galley = ctx.ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));

        let row_h = galley.rows[0].height().max(BAR_H + 2.0);
        let width = match ctx.constraints.x {
            Some(AxisConstraint::Exactly(w)) if w.is_finite() => w,
            Some(AxisConstraint::AtMost(w)) if w.is_finite() => w.min(MAX_WIDTH),
            _ => FALLBACK_WIDTH,
        };

        let origin = ctx.constraints.pos;
        let shown = self.shown();

        // The swatch row, which opens and closes the picker
        let row = ScreenRegion::from_min_size(origin, Vector { x: width, y: row_h });
        let row_resp = ctx.ui.interact(
            row.into(),
            egui::Id::new((ctx.node.id, "row")),
            Sense::CLICK,
        );

        ctx.ui.painter().galley(
            Pos2 {
                x: origin.x,
                y: origin.y + (row_h - galley.rect.height()) * 0.5,
            },
            galley,
            self.text_color.into(),
        );

        let swatch = egui::Rect::from_min_size(
            pos2(origin.x + (width - SWATCH_W).max(0.0), origin.y + 1.0),
            egui::vec2(SWATCH_W.min(width), (row_h - 2.0).max(6.0)),
        );
        // Chequers behind, so a translucent colour reads as translucent.
        egui::color_picker::show_color_at(ctx.ui.painter(), shown.into(), swatch);
        ctx.ui.painter().rect_stroke(
            swatch,
            2.0,
            egui::Stroke::new(
                1.0,
                Color32::from_gray(if row_resp.hovered() { 110 } else { 160 }),
            ),
            egui::StrokeKind::Inside,
        );

        if row_resp.clicked() {
            ctx.submit_action_for_self::<Self, _>(
                SetExpanded { on: !self.expanded },
                "Opened the colour picker",
            );
        }

        if !self.expanded {
            return DrawResult::Complete { region: Some(row) };
        }

        // Picking area
        let mut coords = self.coords_for(shown);
        let opaque = HsvaGamma { a: 1.0, ..coords };
        let mut top = origin.y + row_h + GAP;
        // A gesture that ends anywhere in the picker commits what it chose.
        let mut settled = false;
        let mut moved = false;

        let square = egui::Rect::from_min_size(
            pos2(origin.x, top),
            egui::vec2(width, width * SQUARE_ASPECT),
        );
        let resp = ctx.ui.interact(
            square,
            egui::Id::new((ctx.node.id, "square")),
            Sense::click_and_drag(),
        );
        paint_square(ctx.ui.painter(), square, opaque);
        if let Some(at) = held_position(&resp) {
            coords.s = ((at.x - square.left()) / square.width()).clamp(0.0, 1.0);
            coords.v = 1.0 - ((at.y - square.top()) / square.height()).clamp(0.0, 1.0);
            moved = true;
        }
        settled |= ended(&resp);
        let marker = pos2(
            lerp(square.left()..=square.right(), coords.s),
            lerp(square.top()..=square.bottom(), 1.0 - coords.v),
        );
        ctx.ui.painter().circle_stroke(
            marker,
            4.0,
            egui::Stroke::new(1.5, contrast_with(Color32::from(opaque))),
        );
        top = square.bottom() + GAP;

        let bars: &[Bar] = if self.alpha {
            &[Bar::Hue, Bar::Alpha]
        } else {
            &[Bar::Hue]
        };
        for bar in bars {
            let rect = egui::Rect::from_min_size(pos2(origin.x, top), egui::vec2(width, BAR_H));
            let resp = ctx.ui.interact(
                rect,
                egui::Id::new((ctx.node.id, bar.id_salt())),
                Sense::click_and_drag(),
            );
            paint_bar(ctx.ui.painter(), rect, *bar, opaque);
            let value = match bar {
                Bar::Hue => &mut coords.h,
                Bar::Alpha => &mut coords.a,
            };
            if let Some(at) = held_position(&resp) {
                *value = ((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                moved = true;
            }
            settled |= ended(&resp);
            let x = lerp(rect.left()..=rect.right(), *value);
            ctx.ui.painter().line_segment(
                [pos2(x, rect.top()), pos2(x, rect.bottom())],
                egui::Stroke::new(1.5, Color32::WHITE),
            );
            ctx.ui.painter().rect_stroke(
                rect,
                2.0,
                egui::Stroke::new(1.0, Color32::from_gray(160)),
                egui::StrokeKind::Inside,
            );
            top = rect.bottom() + GAP;
        }

        if moved {
            self.move_to(coords);
        }
        if settled {
            // `live` is left standing until that action lands, so the colour on show never flickers.
            ctx.submit_action_for_self::<Self, _>(
                SetPickedColor {
                    color: self.shown(),
                },
                "Picked a colour",
            );
        }

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(
                origin,
                Vector {
                    x: width,
                    y: (top - GAP) - origin.y,
                },
            )),
        }
    }
}

defhandlers! { ColorPicker {
    actions: [
        SetPickedColor { color: Color } => (this, s) {
            this.color = s.color;
            *this.live.val_mut() = None;
        },
        SetExpanded { on: bool } => (this, s) { this.expanded = s.on },
    ],
    requests: [
        // The colour on show.
        PickedColor => (this, _q): Color { this.shown() },
        // Whether a gesture is still choosing, so a caller can hold off until the user lets go.
        IsPicking => (this, _q): bool { this.live.val().is_some() },
    ],
}}

dex_core::defrequest! {
    /// Show `color` in place of the node's own fill, or drop the preview with `None`.
    PreviewFill { color: Option<Color> } : bool
}

dex_core::defrequest! {
    /// The outline counterpart of [`PreviewFill`].
    PreviewStroke { color: Option<Color> } : bool
}

/// Which of a node's two colours a picker stands for.
#[derive(Clone, Copy)]
pub enum ColorSlot {
    /// The interior, or a label's glyphs.
    Fill,
    /// The outline.
    Stroke,
}

impl ColorSlot {
    /// Show `color` on `target`, or drop the preview with `None`.
    fn preview(self, ws: &Workspace, target: NodeUid, color: Option<Color>) {
        match self {
            Self::Fill => {
                ws.send_request(target, PreviewFill { color });
            }
            Self::Stroke => {
                ws.send_request(target, PreviewStroke { color });
            }
        }
    }
}

fn channels(c: Color) -> [u8; 4] {
    [c.r, c.g, c.b, c.a]
}

/// Drive a picker over the value it stands for, and hand back the colour to commit once the user lets go.
pub fn repicked(
    ws: &Workspace,
    picker: NodeUid<ColorPicker>,
    target: NodeUid,
    slot: ColorSlot,
    actual: Color,
) -> Option<Color> {
    let shown = ws.send_request(picker, PickedColor)?;
    if ws.send_request(picker, IsPicking).unwrap_or(false) {
        slot.preview(ws, target, Some(shown));
        return None;
    }
    if channels(shown) == channels(actual) {
        // The commit has landed; the target shows this colour for real now.
        slot.preview(ws, target, None);
        return None;
    }
    Some(shown)
}

/// Drop any preview a picker left on `target`, for an inspector going away
/// mid-gesture — nothing else would be left to clear it.
pub fn drop_preview(ws: &Workspace, target: NodeUid, slot: ColorSlot) {
    slot.preview(ws, target, None);
}

/// Which gradient a bar shows.
#[derive(Clone, Copy)]
enum Bar {
    Hue,
    Alpha,
}

impl Bar {
    fn id_salt(self) -> &'static str {
        match self {
            Bar::Hue => "hue",
            Bar::Alpha => "alpha",
        }
    }
}

/// Where the pointer is, while it holds `resp` down.
fn held_position(resp: &egui::Response) -> Option<Pos2> {
    (resp.is_pointer_button_down_on() || resp.dragged()).then(|| resp.interact_pointer_pos())?
}

/// Whether the gesture on `resp` has just finished.
fn ended(resp: &egui::Response) -> bool {
    resp.drag_stopped() || resp.clicked()
}

/// A saturation (left to right) and value (bottom to top) field for one hue.
fn paint_square(painter: &Painter, rect: egui::Rect, opaque: HsvaGamma) {
    // Enough subdivisions that the gamma curve does not show as banding.
    const N: usize = 16;
    let mut mesh = Mesh::default();
    for yi in 0..=N {
        for xi in 0..=N {
            let s = xi as f32 / N as f32;
            let v = 1.0 - yi as f32 / N as f32;
            mesh.colored_vertex(
                pos2(
                    lerp(rect.left()..=rect.right(), s),
                    lerp(rect.top()..=rect.bottom(), 1.0 - v),
                ),
                HsvaGamma { s, v, ..opaque }.into(),
            );
            if xi < N && yi < N {
                let at = |x: usize, y: usize| (y * (N + 1) + x) as u32;
                mesh.add_triangle(at(xi, yi), at(xi + 1, yi), at(xi, yi + 1));
                mesh.add_triangle(at(xi + 1, yi), at(xi + 1, yi + 1), at(xi, yi + 1));
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// A left-to-right gradient over one channel of `opaque`.
fn paint_bar(painter: &Painter, rect: egui::Rect, bar: Bar, opaque: HsvaGamma) {
    // Six per hue sector, so every peak hue lands on a vertex.
    const N: usize = 6 * 6;
    if matches!(bar, Bar::Alpha) {
        // Chequers first: an alpha ramp is only legible over something.
        let square = rect.height() * 0.5;
        let mut chequers = Mesh::default();
        let mut x = rect.left();
        let mut column = 0;
        while x < rect.right() {
            let w = square.min(rect.right() - x);
            for row in 0..2 {
                let cell = egui::Rect::from_min_size(
                    pos2(x, rect.top() + row as f32 * square),
                    egui::vec2(
                        w,
                        square.min(rect.bottom() - rect.top() - row as f32 * square),
                    ),
                );
                let shade = if (column + row) % 2 == 0 { 160 } else { 96 };
                chequers.add_colored_rect(cell, Color32::from_gray(shade));
            }
            x += square;
            column += 1;
        }
        painter.add(Shape::mesh(chequers));
    }

    let mut mesh = Mesh::default();
    for i in 0..=N {
        let t = i as f32 / N as f32;
        let color: Color32 = match bar {
            Bar::Hue => HsvaGamma {
                h: t,
                s: 1.0,
                v: 1.0,
                ..opaque
            }
            .into(),
            Bar::Alpha => {
                let solid: Color32 = opaque.into();
                Color32::from_rgba_unmultiplied(solid.r(), solid.g(), solid.b(), (t * 255.0) as u8)
            }
        };
        let x = lerp(rect.left()..=rect.right(), t);
        mesh.colored_vertex(pos2(x, rect.top()), color);
        mesh.colored_vertex(pos2(x, rect.bottom()), color);
        if i < N {
            let at = 2 * i as u32;
            mesh.add_triangle(at, at + 1, at + 2);
            mesh.add_triangle(at + 1, at + 2, at + 3);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Ink that stays visible on top of `background`.
fn contrast_with(background: Color32) -> Color32 {
    if egui::Rgba::from(background).intensity() < 0.5 {
        Color32::WHITE
    } else {
        Color32::BLACK
    }
}
