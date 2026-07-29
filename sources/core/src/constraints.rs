use crate::{ScreenPos, Vector};

#[derive(Clone, Copy)]
pub struct DrawConstraints {
    pub pos: PositionConstraint,
    pub x: Option<AxisConstraint>,
    pub y: Option<AxisConstraint>,
    pub can_request_wrap: bool,
    pub continuation: Option<u64>,
}

impl DrawConstraints {
    pub fn fits(&self, size: Vector) -> bool {
        let x_fits = self.x.is_none_or(|x_ax| x_ax.provided_value() >= size.x);
        let y_fits = self.y.is_none_or(|y_ax| y_ax.provided_value() >= size.y);
        x_fits && y_fits
    }

    pub fn already_wrapped(&self) -> bool {
        self.continuation.is_some()
    }
}

#[derive(Clone, Copy)]
pub enum PositionConstraint {
    Center(ScreenPos),
    TopLeft(ScreenPos),
}

impl PositionConstraint {
    pub fn to_top_left(&self, size: Vector) -> ScreenPos {
        match self {
            Self::TopLeft(tl) => *tl,
            Self::Center(c) => *c - size / 2.0,
        }
    }

    pub fn to_center(&self, size: Vector) -> ScreenPos {
        match self {
            Self::Center(c) => *c,
            Self::TopLeft(tl) => *tl + size / 2.0,
        }
    }
}

#[derive(Clone, Copy)]
pub enum AxisConstraint {
    Exactly(f32),
    AtMost(f32),
}

impl AxisConstraint {
    pub fn provided_value(&self) -> f32 {
        match self {
            Self::Exactly(v) => *v,
            Self::AtMost(v) => *v,
        }
    }
}
