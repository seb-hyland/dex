use egui::{Color32, Mesh, Shape, Stroke, StrokeKind};
use serde::{Deserialize, Serialize};
use workspace::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct Rect {
    pub size: Vector,
    pub corner_radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,
}

#[typetag::serde]
impl Node for Rect {
    fn type_name(&self) -> String {
        "Rectangle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let origin = ctx.constraints.pos.to_top_left(self.size);
        let region = ScreenRegion::from_min_size(origin, self.size);

        let fits = ctx.constraints.fits(self.size);
        if !fits {
            return if ctx.constraints.already_wrapped() {
                DrawResult::Complete { region: None }
            } else {
                DrawResult::Wrap {
                    region: None,
                    continuation: 0,
                }
            };
        }

        ctx.ui.painter().rect(
            region.into(),
            self.corner_radius,
            self.fill_color,
            self.border,
            StrokeKind::Middle,
        );

        DrawResult::Complete {
            region: Some(region),
        }
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Requestable for Rect {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Circle {
    pub radius: f32,
    pub fill_color: Color32,
    pub border: Stroke,
}

#[typetag::serde]
impl Node for Circle {
    fn type_name(&self) -> String {
        "Circle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let size_vector = Vector::splat(self.radius);
        let center = ctx.constraints.pos.to_center(size_vector);

        let bounding_size = Vector::splat(/* Diameter */ self.radius * 2.0);
        let fits = ctx.constraints.fits(bounding_size);

        if !fits {
            return if ctx.constraints.already_wrapped() {
                DrawResult::Complete { region: None }
            } else {
                DrawResult::Wrap {
                    region: None,
                    continuation: 0,
                }
            };
        }

        ctx.ui
            .painter()
            .circle(center.into(), self.radius, self.fill_color, self.border);

        DrawResult::Complete {
            region: Some(ScreenRegion::from_center_size(center, size_vector)),
        }
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Requestable for Circle {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Triangle {
    pub vectors: [Vector; 2],
    pub color: Color32,
}

#[typetag::serde]
impl Node for Triangle {
    fn type_name(&self) -> String {
        "Triangle".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let [vec1, vec2] = self.vectors;
        let bounding_size = Vector::from_points(&[
            ScreenPos::zero(),
            ScreenPos::zero() + vec1,
            ScreenPos::zero() + vec2,
        ]);
        let origin = ctx.constraints.pos.to_top_left(bounding_size);

        let fits = ctx.constraints.fits(bounding_size);
        if !fits {
            return if ctx.constraints.already_wrapped() {
                DrawResult::Complete { region: None }
            } else {
                DrawResult::Wrap {
                    region: None,
                    continuation: 0,
                }
            };
        }

        let mut mesh = Mesh::default();
        let screen_vertices = [origin, origin + vec1, origin + vec2];
        for v in screen_vertices {
            mesh.colored_vertex(v.into(), self.color);
        }
        mesh.add_triangle(0, 1, 2);

        ctx.ui.painter().add(Shape::mesh(mesh));

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_size(origin, bounding_size)),
        }
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Requestable for Triangle {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Line {
    pub span: Vector,
    pub stroke: Stroke,
}

#[typetag::serde]
impl Node for Line {
    fn type_name(&self) -> String {
        "Line".into()
    }

    fn draw(&self, ctx: DrawContext) -> DrawResult {
        let start = ctx.constraints.pos.to_top_left(self.span);
        let end = start + self.span;

        let fits = ctx.constraints.fits(self.span);
        if !fits {
            return if ctx.constraints.already_wrapped() {
                DrawResult::Complete { region: None }
            } else {
                DrawResult::Wrap {
                    region: None,
                    continuation: 0,
                }
            };
        }

        ctx.ui
            .painter()
            .line(vec![start.into(), end.into()], self.stroke);

        DrawResult::Complete {
            region: Some(ScreenRegion::from_min_max(start, end)),
        }
    }

    fn handle_action(&mut self, _r: Box<dyn ActionBody>) {}
}

impl Requestable for Line {
    fn request(&self, _body: Box<dyn RequestBody>) -> Option<Box<dyn std::any::Any>> {
        None
    }
}
