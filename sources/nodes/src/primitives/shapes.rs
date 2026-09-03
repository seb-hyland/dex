use dex_core::prelude::*;
use dex_core::theme;
use egui::Painter;
use utils::Transient;

use crate::layouts::canvas::nodes::CanvasEditor;
use crate::layouts::canvas::nodes::editors::{CircleEditor, PathEditor};
use crate::layouts::vertical::VerticalLayout;
use crate::primitives::color_picker::{
    ColorPicker, ColorSlot, PreviewFill, PreviewStroke, drop_preview, repicked,
};

/// A filled, optionally bordered rectangle.
#[utils::dynamic_type]
#[utils::portable]
pub struct Rect {
    pub size: Vector,
    pub corner_radius: f32,
    pub fill_color: Color,
    pub border: Stroke,
    pub stroke_kind: StrokeKind,
}

#[utils::dynamic_methods]
impl Rect {
    /// A filled rectangle with square corners and no border.
    pub fn new(width: f32, height: f32, fill: Color) -> Self {
        Self {
            size: Vector {
                x: width,
                y: height,
            },
            corner_radius: 0.0,
            fill_color: fill,
            border: Stroke::NONE,
            stroke_kind: StrokeKind::Inside,
        }
    }

    /// A filled rectangle with rounded corners and an inside border.
    pub fn bordered(
        width: f32,
        height: f32,
        fill: Color,
        corner_radius: f32,
        border: Stroke,
    ) -> Self {
        Self {
            size: Vector {
                x: width,
                y: height,
            },
            corner_radius,
            fill_color: fill,
            border,
            stroke_kind: StrokeKind::Inside,
        }
    }
}

impl Rect {
    /**
        The rectangle as one shape, placed with its top-left at `top_left`.

        Fill and border travel together, so a caller that has to reserve a slot
        in the paint list — to put a frame *behind* content it must draw first
        in order to know the frame's size — can fill that slot with this.
    */
    pub fn shape(&self, top_left: ScreenPos) -> egui::Shape {
        egui::epaint::RectShape::new(
            ScreenRegion::from_min_size(top_left, self.size).into(),
            self.corner_radius,
            self.fill_color,
            self.border,
            self.stroke_kind.into(),
        )
        .into()
    }

    /// Paint the rectangle with its top-left corner at `top_left`.
    pub fn paint(&self, painter: &Painter, top_left: ScreenPos) -> ScreenRegion {
        painter.add(self.shape(top_left));
        ScreenRegion::from_min_size(top_left, self.size)
    }
}

#[utils::dynamic_node]
impl Node for Rect {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Rectangle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let region = self.paint(ctx.ui.painter(), ctx.constraints.pos);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { Rect {} }

#[utils::dynamic_type]
#[utils::portable]
pub struct Circle {
    pub radius: f32,
    pub fill_color: Color,
    pub border: Stroke,
}

#[utils::dynamic_methods]
impl Circle {
    /// A filled circle with no border.
    pub fn new(radius: f32, fill: Color) -> Self {
        Self {
            radius,
            fill_color: fill,
            border: Stroke::NONE,
        }
    }

    /// A filled circle with a border.
    pub fn bordered(radius: f32, fill: Color, border: Stroke) -> Self {
        Self {
            radius,
            fill_color: fill,
            border,
        }
    }
}

impl Circle {
    /// Paint the circle centred at `center`.
    pub fn paint(&self, painter: &Painter, center: ScreenPos) -> ScreenRegion {
        let fill: egui::Color32 = self.fill_color.into();
        let border: egui::Stroke = self.border.into();
        painter.circle(center.into(), self.radius, fill, border);
        ScreenRegion::from_center_size(center, Vector::splat(self.radius * 2.0))
    }
}

#[utils::dynamic_node]
impl Node for Circle {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Circle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        // Centre the circle so its bounding box's top-left is the box origin.
        let center = ctx.constraints.pos + Vector::splat(self.radius);
        let region = self.paint(ctx.ui.painter(), center);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { Circle {
    actions: [
        // Set the circle's radius, so an on-canvas editor can resize it.
        SetRadius { radius: f32 } => (this, a) {
            this.radius = a.radius.max(0.0);
        },
    ],
    requests: [
        // The current radius, for an editor to read before resizing.
        GetRadius => (this, _q): f32 { this.radius },
    ],
    extern_requests: [
        // Shared with `Path`, so a converted circle keeps its own colours.
        GetFill => (this, _q): Color { this.fill_color },
        GetStroke => (this, _q): Stroke { this.border },
        // A circle is edited by its centre and radius, not a bounding box.
        CanvasEditor => (_this, q, ctx): NodeUid {
            CircleEditor::build(ctx.workspace.action_handle(), ctx.id, q.canvas_pos, q.size).erase()
        },
    ],
} }

/// One vertex of a [`Path`], with optional cubic-Bézier control handles.
#[utils::dynamic_type]
#[utils::portable]
pub struct Anchor {
    pub pos: Vector,
    pub in_handle: Option<Vector>,
    pub out_handle: Option<Vector>,
}

#[utils::dynamic_methods]
impl Anchor {
    /// A straight corner vertex at `pos`.
    pub fn corner(pos: Vector) -> Self {
        Self {
            pos,
            in_handle: None,
            out_handle: None,
        }
    }

    /// A smooth vertex whose handles mirror each other (`in = -out`).
    pub fn smooth(pos: Vector, out_handle: Vector) -> Self {
        Self {
            pos,
            in_handle: Some(Vector {
                x: -out_handle.x,
                y: -out_handle.y,
            }),
            out_handle: Some(out_handle),
        }
    }
}

/**
   A general vector path.
   Consists of [`Anchor`]s joined by straight or cubic-Bézier segments, optionally closed and filled.
*/
#[utils::dynamic_type]
#[utils::portable]
pub struct Path {
    pub anchors: Vec<Anchor>,
    /// Whether the last point joins back to the first.
    pub closed: bool,
    /// Whether the interior is filled.
    pub filled: bool,
    pub fill: Color,
    pub stroke: Stroke,
    /// Arrowheads at the first / last point (drawn only on an open path).
    pub start_arrow: bool,
    pub end_arrow: bool,

    /// Colours being dragged out of a picker.
    preview_fill: Transient<Color>,
    preview_stroke: Transient<Color>,
}

#[utils::dynamic_methods]
impl Path {
    /// An open, unfilled line from the origin to `(dx, dy)`.
    pub fn line(dx: f32, dy: f32, stroke: Stroke) -> Self {
        Self::span(Vector { x: dx, y: dy }, stroke)
    }

    /// An open, unfilled line from the origin along `span`.
    pub fn span(span: Vector, stroke: Stroke) -> Self {
        Self {
            anchors: vec![Anchor::corner(Vector::ZERO), Anchor::corner(span)],
            closed: false,
            filled: false,
            fill: Color::TRANSPARENT,
            stroke,
            start_arrow: false,
            end_arrow: false,
            preview_fill: Transient::default(),
            preview_stroke: Transient::default(),
        }
    }

    /// A closed, filled polygon through `points` (straight corners).
    pub fn polygon(points: Vec<Vector>, fill: Color, stroke: Stroke) -> Self {
        Self::closed_through(
            points.into_iter().map(Anchor::corner).collect(),
            fill,
            stroke,
        )
    }

    /// A closed, filled polygon through `anchors`.
    pub fn closed_through(anchors: Vec<Anchor>, fill: Color, stroke: Stroke) -> Self {
        Self {
            anchors,
            closed: true,
            filled: true,
            fill,
            stroke,
            start_arrow: false,
            end_arrow: false,
            preview_fill: Transient::default(),
            preview_stroke: Transient::default(),
        }
    }

    /// A closed path tracing the circle of `radius` around `center`.
    pub fn circle(center: Vector, radius: f32, fill: Color, stroke: Stroke) -> Self {
        // 4/3 · tan(π/8), the circle-to-Bézier constant.
        const KAPPA: f32 = 0.552_284_8;
        let k = radius * KAPPA;
        let at = |dx: f32, dy: f32| Vector {
            x: center.x + dx,
            y: center.y + dy,
        };
        // Clockwise from the east point, each handle along the tangent there.
        Self::closed_through(
            vec![
                Anchor::smooth(at(radius, 0.0), Vector { x: 0.0, y: k }),
                Anchor::smooth(at(0.0, radius), Vector { x: -k, y: 0.0 }),
                Anchor::smooth(at(-radius, 0.0), Vector { x: 0.0, y: -k }),
                Anchor::smooth(at(0.0, -radius), Vector { x: k, y: 0.0 }),
            ],
            fill,
            stroke,
        )
    }

    #[dynamic(skip)]
    pub fn default_fill() -> Color {
        Color::rgb(150, 190, 230)
    }

    /// An unfilled outline through `anchors`.
    pub fn unfilled(anchors: Vec<Anchor>, closed: bool, stroke: Stroke) -> Self {
        Self {
            anchors,
            closed,
            filled: false,
            fill: Color::TRANSPARENT,
            stroke,
            start_arrow: false,
            end_arrow: false,
            preview_fill: Transient::default(),
            preview_stroke: Transient::default(),
        }
    }

    /// An open polyline through `points` (straight corners, no fill).
    pub fn polyline(points: Vec<Vector>, stroke: Stroke) -> Self {
        Self::open_through(points.into_iter().map(Anchor::corner).collect(), stroke)
    }

    /// An open, unfilled path through `anchors`, keeping their handles.
    pub fn open_through(anchors: Vec<Anchor>, stroke: Stroke) -> Self {
        Self {
            anchors,
            closed: false,
            filled: false,
            fill: Color::TRANSPARENT,
            stroke,
            start_arrow: false,
            end_arrow: false,
            preview_fill: Transient::default(),
            preview_stroke: Transient::default(),
        }
    }

    #[dynamic(skip)]
    /// Flatten the path into absolute screen points, subdividing Bézier segments.
    fn outline(&self, origin: ScreenPos) -> Vec<egui::Pos2> {
        let n = self.anchors.len();
        if n == 0 {
            return Vec::new();
        }
        let at = |v: Vector| -> egui::Pos2 { (origin + v).into() };
        let mut points = vec![at(self.anchors[0].pos)];
        let segments = if self.closed { n } else { n - 1 };
        for i in 0..segments {
            let a = &self.anchors[i];
            let b = &self.anchors[(i + 1) % n];
            match (a.out_handle, b.in_handle) {
                (None, None) => points.push(at(b.pos)),
                (out, inc) => {
                    let control = [
                        at(a.pos),
                        at(a.pos + out.unwrap_or_default()),
                        at(b.pos + inc.unwrap_or_default()),
                        at(b.pos),
                    ];
                    let bezier = egui::epaint::CubicBezierShape::from_points_stroke(
                        control,
                        false,
                        egui::Color32::TRANSPARENT,
                        egui::Stroke::NONE,
                    );
                    // Skip the first flattened point; it repeats the previous endpoint.
                    points.extend(bezier.flatten(Some(0.1)).into_iter().skip(1));
                }
            }
        }
        points
    }

    #[dynamic(skip)]
    /// The interior colour on show: one being picked, else the committed one.
    pub fn shown_fill(&self) -> Color {
        self.preview_fill.val().unwrap_or(self.fill)
    }

    #[dynamic(skip)]
    /// The outline on show, its colour possibly still being picked.
    pub fn shown_stroke(&self) -> Stroke {
        match *self.preview_stroke.val() {
            Some(color) => Stroke {
                color,
                ..self.stroke
            },
            None => self.stroke,
        }
    }

    #[dynamic(skip)]
    /// Paint the path with its anchor origin at `origin`.
    pub fn paint(&self, painter: &Painter, origin: ScreenPos) -> ScreenRegion {
        let points = self.outline(origin);
        if points.is_empty() {
            return ScreenRegion::from_min_size(origin, Vector::ZERO);
        }
        let region = egui::Rect::from_points(&points);
        let shown_stroke = self.shown_stroke();
        let stroke: egui::Stroke = shown_stroke.into();

        // Fill the interior when asked, closed or not, using ear clipping.
        if self.filled {
            let fill: egui::Color32 = self.shown_fill().into();
            if fill.a() > 0 {
                painter.add(egui::Shape::mesh(fill_mesh(&points, fill)));
            }
        }
        // epaint needs two points to stroke anything, and asserts on fewer.
        if points.len() < 2 {
            return region.into();
        }
        if self.closed {
            painter.add(egui::epaint::PathShape::closed_line(points.clone(), stroke));
        } else {
            // Stop the stroke short of any head.
            let head = arrowhead_size(shown_stroke.width);
            let mut line = points.clone();
            if self.end_arrow {
                line = retracted(&line, head * HEAD_OVERLAP);
            }
            if self.start_arrow {
                line.reverse();
                line = retracted(&line, head * HEAD_OVERLAP);
                line.reverse();
            }
            if line.len() >= 2 {
                painter.add(egui::epaint::PathShape::line(line, stroke));
            }
        }

        // Arrowheads on the open ends, tipped at the outline's real endpoints.
        if !self.closed && points.len() >= 2 {
            let color: egui::Color32 = shown_stroke.color.into();
            let size = arrowhead_size(shown_stroke.width);
            if self.end_arrow {
                paint_arrowhead(
                    painter,
                    points[points.len() - 2],
                    points[points.len() - 1],
                    color,
                    size,
                );
            }
            if self.start_arrow {
                paint_arrowhead(painter, points[1], points[0], color, size);
            }
        }
        region.into()
    }
}

/// How long an arrowhead is, for a line of `stroke_width`.
fn arrowhead_size(stroke_width: f32) -> f32 {
    8.0 + stroke_width * 1.5
}

/// How far into a head the line is allowed to run, as a fraction of the head's length.
const HEAD_OVERLAP: f32 = 0.6;

/// `points` with `dist` of length taken off its far end.
fn retracted(points: &[egui::Pos2], dist: f32) -> Vec<egui::Pos2> {
    let mut out = points.to_vec();
    let mut left = dist;
    while out.len() >= 2 {
        let n = out.len();
        let (a, b) = (out[n - 2], out[n - 1]);
        let seg = (b - a).length();
        if seg >= left {
            if seg > 1e-6 {
                out[n - 1] = a + (b - a) * ((seg - left) / seg);
            }
            break;
        }
        left -= seg;
        out.pop();
    }
    out
}

/// Paint a filled triangular arrowhead at `tip`, pointing away from `from`.
fn paint_arrowhead(
    painter: &Painter,
    from: egui::Pos2,
    tip: egui::Pos2,
    color: egui::Color32,
    size: f32,
) {
    let dir = tip - from;
    let len = dir.length();
    if len < 1e-3 {
        return;
    }
    let dir = dir / len;
    let perp = egui::vec2(-dir.y, dir.x);
    let base = tip - dir * size;
    let half = size * 0.5;
    let p1 = base + perp * half;
    let p2 = base - perp * half;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, p1, p2],
        color,
        egui::Stroke::NONE,
    ));
}

/// Triangulate a simple polygon into a filled mesh via ear clipping.
fn fill_mesh(points: &[egui::Pos2], color: egui::Color32) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    let n = points.len();
    if n < 3 {
        return mesh;
    }
    for &p in points {
        mesh.colored_vertex(p, color);
    }

    // Signed cross product of `oa` and `ob`; positive when `o->a->b` turns left.
    let cross = |o: egui::Pos2, a: egui::Pos2, b: egui::Pos2| {
        (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
    };
    // Whether `p` lies within triangle `abc` (edges inclusive).
    let in_tri = |p: egui::Pos2, a: egui::Pos2, b: egui::Pos2, c: egui::Pos2| {
        let (d1, d2, d3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    };

    // Build a vertex ring, dropping points coincident with their predecessor and a final point equal to the first.
    let same = |a: egui::Pos2, b: egui::Pos2| (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4;
    let mut ring: Vec<usize> = Vec::with_capacity(n);
    for i in 0..n {
        if let Some(&last) = ring.last()
            && same(points[i], points[last])
        {
            continue;
        }
        ring.push(i);
    }
    if ring.len() >= 2 && same(points[ring[0]], points[*ring.last().unwrap()]) {
        ring.pop();
    }
    if ring.len() < 3 {
        return mesh;
    }

    // Wind the ring counter-clockwise, so a left turn marks a convex ear.
    let twice_area: f32 = ring
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let (a, b) = (points[i], points[ring[(k + 1) % ring.len()]]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    if twice_area < 0.0 {
        ring.reverse();
    }

    while ring.len() > 3 {
        let m = ring.len();
        let mut clipped = false;
        for i in 0..m {
            let ia = ring[(i + m - 1) % m];
            let ib = ring[i];
            let ic = ring[(i + 1) % m];
            let (a, b, c) = (points[ia], points[ib], points[ic]);
            if cross(a, b, c) <= 0.0 {
                continue; // reflex or straight vertex: not an ear tip.
            }
            let blocked = ring
                .iter()
                .any(|&ip| ip != ia && ip != ib && ip != ic && in_tri(points[ip], a, b, c));
            if blocked {
                continue;
            }
            mesh.add_triangle(ia as u32, ib as u32, ic as u32);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break; // no ear found: outline is degenerate or self-intersecting.
        }
    }
    if ring.len() == 3 {
        mesh.add_triangle(ring[0] as u32, ring[1] as u32, ring[2] as u32);
    }
    mesh
}

#[utils::dynamic_node]
impl Node for Path {
    fn type_name(&self, _ctx: NodeContext) -> String {
        if self.closed {
            "A Polygon".into()
        } else {
            "A Path".into()
        }
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let region = self.paint(ctx.ui.painter(), ctx.constraints.pos);
        DrawResult::Complete {
            region: Some(region),
        }
    }
    fn build_inspector(&self, ctx: NodeContext) -> Option<NodeUid> {
        Some(PathMenu::build(ctx.workspace, ctx.id, self.filled).erase())
    }
}

defhandlers! { Path {
    actions: [
        // Replace the whole anchor list.
        SetAnchors { anchors: Vec<Anchor> } => (this, a) {
            this.anchors = a.anchors;
        },
        // Whether the last point joins the first.
        SetPathClosed { closed: bool } => (this, a) {
            this.closed = a.closed;
        },
        // Whether the interior is filled (independent of `closed`).
        SetPathFilled { filled: bool } => (this, a) {
            this.filled = a.filled;
        },
        SetPathFill { color: Color } => (this, a) {
            this.fill = a.color;
        },
        SetPathStrokeColor { color: Color } => (this, a) {
            this.stroke.color = a.color;
        },
        // Line weight, so a path that scales with its container can keep its
        // strokes in proportion.
        SetPathStrokeWidth { width: f32 } => (this, a) {
            this.stroke.width = a.width;
        },
        SetPathArrows { start: bool, end: bool } => (this, a) {
            this.start_arrow = a.start;
            this.end_arrow = a.end;
        },
    ],
    requests: [
        // The current anchors, for an editor to read before mutating them.
        GetAnchors => (this, _q): Vec<Anchor> { this.anchors.clone() },
        IsPathClosed => (this, _q): bool { this.closed },
        IsPathFilled => (this, _q): bool { this.filled },
        GetFill => (this, _q): Color { this.fill },
        GetStroke => (this, _q): Stroke { this.stroke },
        HasStartArrow => (this, _q): bool { this.start_arrow },
        HasEndArrow => (this, _q): bool { this.end_arrow },
    ],
    extern_requests: [
        PreviewFill => (this, q): bool {
            match q.color {
                Some(color) => this.preview_fill.set(color),
                None => *this.preview_fill.val_mut() = None,
            }
            true
        },
        PreviewStroke => (this, q): bool {
            match q.color {
                Some(color) => this.preview_stroke.set(color),
                None => *this.preview_stroke.val_mut() = None,
            }
            true
        },
        // A path is edited by dragging its anchors, not a bounding box.
        CanvasEditor => (this, q, ctx): NodeUid {
            // A two-point open path is a line; anything else edits as a polygon.
            let is_line = !this.closed && this.anchors.len() == 2;
            // An open path is something still being drawn, so its points start live.
            // A closed one is a finished shape, moved as a whole until its inspector says otherwise.
            PathEditor::build(
                ctx.workspace.action_handle(),
                ctx.id,
                q.canvas_pos,
                is_line,
                !this.closed,
            )
            .erase()
        },
    ],
} }

#[cfg(test)]
mod tests {
    use super::*;

    /// A cubic Bézier at `t`, to check the traced outline against the circle it
    /// is meant to reproduce.
    fn bezier(p0: Vector, c1: Vector, c2: Vector, p3: Vector, t: f32) -> Vector {
        let u = 1.0 - t;
        let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        Vector {
            x: a * p0.x + b * c1.x + c * c2.x + d * p3.x,
            y: a * p0.y + b * c1.y + c * c2.y + d * p3.y,
        }
    }

    /// Every point of a converted circle must sit on the circle it came from,
    /// or the shape jumps the moment it is converted.
    #[test]
    fn a_traced_circle_stays_on_the_circle() {
        let (center, radius) = (Vector { x: 50.0, y: 50.0 }, 40.0);
        let path = Path::circle(center, radius, Color::WHITE, Stroke::NONE);
        assert_eq!(path.anchors.len(), 4, "one anchor per quarter arc");
        assert!(path.closed && path.filled);

        for i in 0..4 {
            let a = &path.anchors[i];
            let b = &path.anchors[(i + 1) % 4];
            let c1 = a.pos + a.out_handle.expect("smooth");
            let c2 = b.pos + b.in_handle.expect("smooth");
            for step in 0..=8 {
                let t = step as f32 / 8.0;
                let p = bezier(a.pos, c1, c2, b.pos, t);
                let d = ((p.x - center.x).powi(2) + (p.y - center.y).powi(2)).sqrt();
                assert!(
                    (d - radius).abs() < radius * 1e-3,
                    "arc {i} at t={t} is {d} from the centre, not {radius}"
                );
            }
        }
    }

    /// Where the line stops, the head must already be at least as wide as the
    /// line — otherwise the line's shoulders show past the point, which is what
    /// left a stub of line poking out of the tip.
    #[test]
    fn the_line_stops_where_the_head_is_wider_than_it() {
        for width in [0.5, 1.0, 2.0, 2.5, 4.0, 8.0, 16.0] {
            let head = arrowhead_size(width);
            let stops_at = head * HEAD_OVERLAP;
            // The head is a triangle from `head / 2` wide at its base to a
            // point, so at `d` from the tip it is `d / 2` to either side.
            let head_half_width = stops_at / 2.0;
            assert!(
                head_half_width >= width / 2.0,
                "at width {width} the line stops where the head is only \
                 {head_half_width} wide, against the line's {}",
                width / 2.0
            );
            assert!(stops_at < head, "and it still stops inside the head");
        }
    }

    #[test]
    fn retracting_takes_the_asked_for_length_off_the_end() {
        let line = vec![egui::pos2(0.0, 0.0), egui::pos2(100.0, 0.0)];
        let out = retracted(&line, 30.0);
        assert_eq!(out.len(), 2);
        assert!((out[1].x - 70.0).abs() < 1e-3 && out[1].y.abs() < 1e-3);
    }

    /// A flattened curve is a great many short segments, so the walk has to
    /// cross as many of them as the distance takes.
    #[test]
    fn retracting_walks_back_through_short_segments() {
        let line: Vec<egui::Pos2> = (0..=20).map(|i| egui::pos2(i as f32 * 5.0, 0.0)).collect();
        let out = retracted(&line, 32.0);
        assert!((out.last().unwrap().x - 68.0).abs() < 1e-3, "100 - 32");
        assert!(out.len() < line.len(), "it dropped the points it passed");
        assert!(out.len() >= 2, "and kept a drawable line");
    }

    /// Trimming more than there is leaves a point, not a panic or a wrap-around.
    #[test]
    fn retracting_past_the_whole_line_leaves_one_point() {
        let line = vec![egui::pos2(0.0, 0.0), egui::pos2(10.0, 0.0)];
        let out = retracted(&line, 40.0);
        assert_eq!(out.len(), 1);
        assert!((out[0].x).abs() < 1e-3);
    }

    /// The handles mirror each other at every anchor, so the outline is smooth
    /// across the joins rather than kinked at the cardinal points.
    #[test]
    fn a_traced_circle_is_smooth_at_its_anchors() {
        let path = Path::circle(Vector::ZERO, 30.0, Color::WHITE, Stroke::NONE);
        for a in &path.anchors {
            let (i, o) = (a.in_handle.expect("smooth"), a.out_handle.expect("smooth"));
            assert!((i.x + o.x).abs() < 1e-4 && (i.y + o.y).abs() < 1e-4);
        }
    }
}

/// A [`Path`]'s inspector: the colours it is drawn in.
#[utils::portable]
pub struct PathMenu {
    #[uid_ref]
    target: NodeUid<Path>,
    column: NodeUid<VerticalLayout>,
    stroke_picker: NodeUid<ColorPicker>,
    fill_picker: Option<NodeUid<ColorPicker>>,
}

impl PathMenu {
    fn build(ws: &Workspace, target: NodeUid, filled: bool) -> NodeUid<PathMenu> {
        let h = ws.action_handle();
        // Seeded from the path as it stands, so the swatch opens showing the
        // colour it is about to change rather than a default.
        let stroke = ws.send_request(target, GetStroke).unwrap_or(Stroke::NONE);
        let stroke_picker = ColorPicker::build(h.clone(), "Stroke".into(), stroke.color);
        let fill_picker = filled.then(|| {
            let fill = ws
                .send_request(target, GetFill)
                .unwrap_or(Color::TRANSPARENT);
            ColorPicker::build(h.clone(), "Fill".into(), fill)
        });

        let mut rows = vec![stroke_picker.erase()];
        rows.extend(fill_picker.map(NodeUid::erase));
        let column = VerticalLayout::build(h, rows, theme::SPACE_XS);
        ws.insert_node(Self {
            target: target.cast(),
            column,
            stroke_picker,
            fill_picker,
        })
    }
}

#[utils::dynamic_node(skip)]
impl Node for PathMenu {
    fn type_name(&self, _ctx: NodeContext) -> String {
        "A Path Menu".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let constraints = ctx.constraints;
        let drawn = ctx.draw_workspace_node(self.column.erase(), constraints);
        let ws = ctx.node.workspace;
        let target = self.target.erase();

        if let Some(stroke) = ws.send_request(target, GetStroke)
            && let Some(color) = repicked(
                ws,
                self.stroke_picker,
                target,
                ColorSlot::Stroke,
                stroke.color,
            )
        {
            ws.submit_action(target, "Set stroke colour", SetPathStrokeColor { color });
        }
        if let Some(picker) = self.fill_picker
            && let Some(fill) = ws.send_request(target, GetFill)
            && let Some(color) = repicked(ws, picker, target, ColorSlot::Fill, fill)
        {
            ws.submit_action(target, "Set fill colour", SetPathFill { color });
        }

        drawn.unwrap_or(DrawResult::Complete { region: None })
    }

    fn on_delete(&self, ctx: NodeContext) {
        // The menu is going away mid-gesture if a preview is still showing; drop it.
        drop_preview(ctx.workspace, self.target.erase(), ColorSlot::Stroke);
        if self.fill_picker.is_some() {
            drop_preview(ctx.workspace, self.target.erase(), ColorSlot::Fill);
        }
        ctx.workspace.delete_node(self.column.erase());
        ctx.workspace.delete_node(self.stroke_picker.erase());
        if let Some(picker) = self.fill_picker {
            ctx.workspace.delete_node(picker.erase());
        }
    }
}

defhandlers! { PathMenu {} }
