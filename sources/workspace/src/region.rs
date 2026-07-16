use egui::{Pos2, Rect};
use serde::{Deserialize, Serialize};

/**
   A region in screen-space.
   Multiple variants are provided for differently-shaped regions.
*/
#[derive(Clone, Serialize, Deserialize)]
pub enum DrawRegion {
    Rect(Rect),
    Circle(Circle),
    Ellipse(Ellipse),
    Line(Line),
    Path(Path),
    QBezier(QBezier),
    CBezier(CBezier),
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Circle {
    pub center: Pos2,
    pub radius: f32,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Ellipse {
    pub center: Pos2,
    pub height: f32,
    pub width: f32,
    pub angle: f32,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Line {
    pub points: [Pos2; 2],
    pub width: f32,
    pub directed: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Path {
    pub points: Vec<Pos2>,
    pub width: f32,
    pub directed: bool,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct QBezier {
    pub points: [Pos2; 3],
    pub width: f32,
    pub directed: bool,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CBezier {
    pub points: [Pos2; 4],
    pub width: f32,
    pub directed: bool,
}
