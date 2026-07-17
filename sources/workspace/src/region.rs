use egui::{Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};

/**
    A position in screen-space.
*/
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ScreenPos {
    pub x: f32,
    pub y: f32,
}

impl From<ScreenPos> for Pos2 {
    fn from(value: ScreenPos) -> Self {
        let ScreenPos { x, y } = value;
        Self { x, y }
    }
}

impl From<Pos2> for ScreenPos {
    fn from(value: Pos2) -> Self {
        let Pos2 { x, y } = value;
        Self { x, y }
    }
}

impl ScreenPos {
    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/**
   A rectangular region in screen-space.
*/
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct ScreenRegion {
    pub min: ScreenPos,
    pub max: ScreenPos,
}

impl From<ScreenRegion> for Rect {
    fn from(value: ScreenRegion) -> Self {
        let ScreenRegion { min, max } = value;
        Self {
            min: min.into(),
            max: max.into(),
        }
    }
}

impl From<Rect> for ScreenRegion {
    fn from(value: Rect) -> Self {
        let Rect { min, max } = value;
        Self {
            min: min.into(),
            max: max.into(),
        }
    }
}

impl ScreenRegion {
    pub fn empty() -> Self {
        Self {
            min: ScreenPos::zero(),
            max: ScreenPos::zero(),
        }
    }
}

/**
   A 2-dimensional vector.
*/
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
}

impl From<Vector> for Vec2 {
    fn from(value: Vector) -> Self {
        let Vector { x, y } = value;
        Self { x, y }
    }
}
