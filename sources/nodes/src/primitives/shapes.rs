use dex_core::prelude::*;
use egui::Painter;

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
    /// Paint the rectangle with its top-left corner at `top_left`.
    pub fn paint(&self, painter: &Painter, top_left: ScreenPos) -> ScreenRegion {
        let region = ScreenRegion::from_min_size(top_left, self.size);
        let fill: egui::Color32 = self.fill_color.into();
        let border: egui::Stroke = self.border.into();
        let stroke_kind: egui::StrokeKind = self.stroke_kind.into();

        painter.rect(region.into(), self.corner_radius, fill, border, stroke_kind);
        region
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

defhandlers! { Circle {} }

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
    pub closed: bool,
    /// Interior fill. Ignored while the path is open (`closed` is false).
    pub fill: Color,
    pub stroke: Stroke,
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
            anchors: vec![
                Anchor::corner(Vector { x: 0.0, y: 0.0 }),
                Anchor::corner(span),
            ],
            closed: false,
            fill: Color::TRANSPARENT,
            stroke,
        }
    }

    /// A closed, filled polygon through `points` (straight corners).
    pub fn polygon(points: Vec<Vector>, fill: Color, stroke: Stroke) -> Self {
        Self {
            anchors: points.into_iter().map(Anchor::corner).collect(),
            closed: true,
            fill,
            stroke,
        }
    }

    /// An open polyline through `points` (straight corners, no fill).
    pub fn polyline(points: Vec<Vector>, stroke: Stroke) -> Self {
        Self {
            anchors: points.into_iter().map(Anchor::corner).collect(),
            closed: false,
            fill: Color::TRANSPARENT,
            stroke,
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
                    let zero = Vector { x: 0.0, y: 0.0 };
                    let control = [
                        at(a.pos),
                        at(a.pos + out.unwrap_or(zero)),
                        at(b.pos + inc.unwrap_or(zero)),
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
    /// Paint the path with its anchor origin at `origin`.
    pub fn paint(&self, painter: &Painter, origin: ScreenPos) -> ScreenRegion {
        let points = self.outline(origin);
        if points.is_empty() {
            return ScreenRegion::from_min_size(origin, Vector { x: 0.0, y: 0.0 });
        }
        let region = egui::Rect::from_points(&points);
        let stroke: egui::Stroke = self.stroke.into();
        if self.closed {
            // Fill via ear clipping so concave shapes render correctly.
            let fill: egui::Color32 = self.fill.into();
            if fill.a() > 0 {
                painter.add(egui::Shape::mesh(fill_mesh(&points, fill)));
            }
            painter.add(egui::epaint::PathShape::closed_line(points, stroke));
        } else {
            painter.add(egui::epaint::PathShape::line(points, stroke));
        }
        region.into()
    }
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
}

defhandlers! { Path {} }
