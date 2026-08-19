use dex_core::prelude::*;
use egui::{Color32, Mesh, Painter, Shape, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use utils::Reset;

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Rect {
    pub size: Vector,
    pub corner_radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,
    pub stroke_kind: StrokeKind,
}

impl Rect {
    /// Paint the rectangle with its top-left corner at `top_left`.
    pub fn paint(&self, painter: &Painter, top_left: ScreenPos) -> ScreenRegion {
        let region = ScreenRegion::from_min_size(top_left, self.size);
        painter.rect(
            region.into(),
            self.corner_radius,
            self.fill_color,
            self.border,
            self.stroke_kind,
        );
        region
    }
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Circle {
    pub radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,
}

impl Circle {
    /// Paint the circle centred at `center`.
    pub fn paint(&self, painter: &Painter, center: ScreenPos) -> ScreenRegion {
        painter.circle(center.into(), self.radius, self.fill_color, self.border);
        ScreenRegion::from_center_size(center, Vector::splat(self.radius * 2.0))
    }
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Triangle {
    pub vectors: [Vector; 2],
    pub color: Color32,
}

impl Triangle {
    /// Paint the triangle with its first vertex at `origin` and the other two at
    /// `origin + vectors[0]` and `origin + vectors[1]`.
    pub fn paint(&self, painter: &Painter, origin: ScreenPos) -> ScreenRegion {
        let [vec1, vec2] = self.vectors;
        let mut mesh = Mesh::default();
        for v in [origin, origin + vec1, origin + vec2] {
            mesh.colored_vertex(v.into(), self.color);
        }
        mesh.add_triangle(0, 1, 2);
        painter.add(Shape::mesh(mesh));

        let bounding = Vector::from_points(&[origin, origin + vec1, origin + vec2]);
        ScreenRegion::from_min_size(origin, bounding)
    }
}

#[derive(Clone, Reset, Serialize, Deserialize)]
pub struct Line {
    pub span: Vector,
    pub stroke: Stroke,
}

impl Line {
    /// Paint the line from `start` to `start + span`.
    pub fn paint(&self, painter: &Painter, start: ScreenPos) -> ScreenRegion {
        let end = start + self.span;
        painter.line(vec![start.into(), end.into()], self.stroke);
        ScreenRegion::from_min_max(start, end)
    }
}
