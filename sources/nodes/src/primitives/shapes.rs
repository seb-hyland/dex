use dex_core::prelude::*;
use egui::{Mesh, Painter, Shape};

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
    fn type_name(&self) -> String {
        "Rectangle".into()
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
    fn type_name(&self) -> String {
        "Circle".into()
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

/// A filled triangle, given as two edge vectors from a shared origin vertex.
#[utils::dynamic_type]
#[utils::portable]
pub struct Triangle {
    pub vectors: [Vector; 2],
    pub color: Color,
}

#[utils::dynamic_methods]
impl Triangle {
    /// A triangle with its first vertex at the origin and the other two at
    /// `(ax, ay)` and `(bx, by)` relative to it.
    pub fn new(ax: f32, ay: f32, bx: f32, by: f32, color: Color) -> Self {
        Self {
            vectors: [Vector { x: ax, y: ay }, Vector { x: bx, y: by }],
            color,
        }
    }
}

impl Triangle {
    /// Paint the triangle with its first vertex at `origin` and the other two at
    /// `origin + vectors[0]` and `origin + vectors[1]`.
    pub fn paint(&self, painter: &Painter, origin: ScreenPos) -> ScreenRegion {
        let [vec1, vec2] = self.vectors;
        let color: egui::Color32 = self.color.into();
        let mut mesh = Mesh::default();
        for v in [origin, origin + vec1, origin + vec2] {
            mesh.colored_vertex(v.into(), color);
        }
        mesh.add_triangle(0, 1, 2);
        painter.add(Shape::mesh(mesh));

        let bounding = Vector::from_points(&[origin, origin + vec1, origin + vec2]);
        ScreenRegion::from_min_size(origin, bounding)
    }
}

#[utils::dynamic_node]
impl Node for Triangle {
    fn type_name(&self) -> String {
        "Triangle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let region = self.paint(ctx.ui.painter(), ctx.constraints.pos);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { Triangle {} }

/// A straight line from an origin along a span vector.
#[utils::dynamic_type]
#[utils::portable]
pub struct Line {
    pub span: Vector,
    pub stroke: Stroke,
}

#[utils::dynamic_methods]
impl Line {
    /// A line spanning `(dx, dy)` from its origin, in the given stroke.
    pub fn new(dx: f32, dy: f32, stroke: Stroke) -> Self {
        Self {
            span: Vector { x: dx, y: dy },
            stroke,
        }
    }
}

impl Line {
    /// Paint the line from `start` to `start + span`.
    pub fn paint(&self, painter: &Painter, start: ScreenPos) -> ScreenRegion {
        let end = start + self.span;
        let stroke: egui::Stroke = self.stroke.into();
        painter.line(vec![start.into(), end.into()], stroke);
        ScreenRegion::from_min_max(start, end)
    }
}

#[utils::dynamic_node]
impl Node for Line {
    fn type_name(&self) -> String {
        "Line".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let region = self.paint(ctx.ui.painter(), ctx.constraints.pos);
        DrawResult::Complete {
            region: Some(region),
        }
    }
}

defhandlers! { Line {} }
