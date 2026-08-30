use std::ops::{Add, Div, Sub};

use egui::{Pos2, Rect, Vec2};

/**
    A position in screen-space.
*/
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
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

impl Add for ScreenPos {
    type Output = ScreenPos;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for ScreenPos {
    type Output = ScreenPos;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Add<Vector> for ScreenPos {
    type Output = ScreenPos;

    fn add(self, rhs: Vector) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub<Vector> for ScreenPos {
    type Output = ScreenPos;

    fn sub(self, rhs: Vector) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

#[utils::dynamic_methods]
impl ScreenPos {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    pub fn to_vector(self) -> Vector {
        Vector {
            x: self.x,
            y: self.y,
        }
    }
}

/**
   A rectangular region in screen-space.
*/
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable]
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

#[utils::dynamic_methods]
impl ScreenRegion {
    pub fn empty() -> Self {
        Self {
            min: ScreenPos::zero(),
            max: ScreenPos::zero(),
        }
    }

    pub fn from_center_size(center: ScreenPos, size: Vector) -> Self {
        Self::from(Rect::from_center_size(center.into(), size.into()))
    }

    pub fn from_min_size(min: ScreenPos, size: Vector) -> Self {
        Self::from(Rect::from_min_size(min.into(), size.into()))
    }

    pub fn from_min_max(min: ScreenPos, max: ScreenPos) -> Self {
        Self::from(Rect::from_min_max(min.into(), max.into()))
    }

    pub fn size(&self) -> Vector {
        Rect::from(*self).size().into()
    }

    /// Whether `point` falls inside this region.
    pub fn contains(self, point: ScreenPos) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    pub fn union(self, other: ScreenRegion) -> Self {
        Self::from(Rect::from(self).union(other.into()))
    }

    pub fn intersect(self, other: ScreenRegion) -> Option<Self> {
        let origin_rect = Rect::from(self);
        let clip_rect = Rect::from(other);

        if !origin_rect.intersects(clip_rect) {
            None
        } else {
            Some(Self::from(origin_rect.intersect(clip_rect)))
        }
    }
}

/**
   A 2-dimensional vector.
*/
#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable(noop_reset)]
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

impl From<Vec2> for Vector {
    fn from(value: Vec2) -> Self {
        let Vec2 { x, y } = value;
        Self { x, y }
    }
}

impl Add for Vector {
    type Output = Vector;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Add<f32> for Vector {
    type Output = Vector;

    fn add(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x + rhs,
            y: self.y + rhs,
        }
    }
}

impl Sub for Vector {
    type Output = Vector;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Div<f32> for Vector {
    type Output = Vector;

    fn div(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

#[utils::dynamic_methods]
impl Vector {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn splat(s: f32) -> Self {
        Self { x: s, y: s }
    }

    pub fn from_points(points: &[ScreenPos]) -> Vector {
        let pos2_points: Vec<_> = points.iter().map(|p| Pos2::from(*p)).collect();
        let rect = Rect::from_points(&pos2_points);
        rect.size().into()
    }

    pub fn to_screen_pos(self) -> ScreenPos {
        ScreenPos {
            x: self.x,
            y: self.y,
        }
    }

    pub fn map(self, mut f: impl FnMut(f32) -> f32) -> Self {
        Self {
            x: f(self.x),
            y: f(self.y),
        }
    }
}
