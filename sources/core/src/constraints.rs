use std::ops::{Add, Sub};

use crate::{ScreenPos, Vector};

#[derive(Copy)]
#[utils::dynamic_type(new)]
#[utils::portable]
pub struct DrawConstraints {
    /// Top-left of the region to draw into.
    pub pos: ScreenPos,
    pub x: Option<AxisConstraint>,
    pub y: Option<AxisConstraint>,
    pub wrap: WrapConstraints,
    pub should_clip: bool,
}

#[utils::dynamic_methods]
impl DrawConstraints {
    /// The room on offer, on both axes at once. An unbounded axis reads as infinite.
    pub fn available(&self) -> Vector {
        Vector {
            x: self.x.map_or(f32::INFINITY, |ax| ax.provided_value()),
            y: self.y.map_or(f32::INFINITY, |ax| ax.provided_value()),
        }
    }

    pub fn fits(&self, size: Vector) -> bool {
        let x_fits = self.x.is_none_or(|x_ax| x_ax.provided_value() >= size.x);
        let y_fits = self.y.is_none_or(|y_ax| y_ax.provided_value() >= size.y);
        x_fits && y_fits
    }

    pub fn shrunk_by_per_side(self, x: f32, y: f32) -> Self {
        Self {
            pos: self.pos + ScreenPos { x, y },
            x: self.x.map(|x_ax| x_ax - 2.0 * x),
            y: self.y.map(|y_ax| y_ax - 2.0 * y),
            wrap: self.wrap,
            should_clip: self.should_clip,
        }
    }
}

#[derive(Copy)]
#[utils::dynamic_type]
#[utils::portable]
pub enum AxisConstraint {
    Exactly(f32),
    AtMost(f32),
}

#[utils::dynamic_methods]
impl AxisConstraint {
    pub fn provided_value(&self) -> f32 {
        match self {
            Self::Exactly(v) => *v,
            Self::AtMost(v) => *v,
        }
    }
}

impl Add<f32> for AxisConstraint {
    type Output = Self;

    fn add(self, rhs: f32) -> Self::Output {
        match self {
            Self::Exactly(v) => Self::Exactly(v + rhs),
            Self::AtMost(v) => Self::AtMost(v + rhs),
        }
    }
}

impl Sub<f32> for AxisConstraint {
    type Output = Self;

    fn sub(self, rhs: f32) -> Self::Output {
        match self {
            Self::Exactly(v) => Self::Exactly(v - rhs),
            Self::AtMost(v) => Self::AtMost(v - rhs),
        }
    }
}

#[derive(Copy)]
#[utils::portable]
pub enum WrapConstraints {
    CanRequest {
        at_start_of_line: bool,
        continuation: Option<u64>,
    },
    NotAllowed,
}

// Not `#[dynamic_methods]`: `WrapConstraints` is not a pyclass (see its
// hand-written boundary mapping below), so it carries no Python methods. A
// script inspects the `None | (at_start_of_line, continuation)` form directly.
impl WrapConstraints {
    pub fn can_retry_on_newline(&self) -> bool {
        match self {
            Self::CanRequest {
                at_start_of_line, ..
            } => !*at_start_of_line, // Do not allow retry on newline if already at start of line
            Self::NotAllowed => false,
        }
    }
}

/**
    A hand-written boundary mapping; `WrapConstraints` cannot be mapped automatically by pyo3.
*/
impl<'py> pyo3::IntoPyObject<'py> for WrapConstraints {
    type Target = pyo3::PyAny;
    type Output = pyo3::Bound<'py, pyo3::PyAny>;
    type Error = pyo3::PyErr;

    fn into_pyobject(self, py: pyo3::Python<'py>) -> Result<Self::Output, Self::Error> {
        let repr = match self {
            Self::NotAllowed => None,
            Self::CanRequest {
                at_start_of_line,
                continuation,
            } => Some((at_start_of_line, continuation)),
        };
        pyo3::IntoPyObjectExt::into_bound_py_any(repr, py)
    }
}

impl<'a, 'py> pyo3::FromPyObject<'a, 'py> for WrapConstraints {
    type Error = pyo3::PyErr;

    fn extract(ob: pyo3::Borrowed<'a, 'py, pyo3::PyAny>) -> Result<Self, Self::Error> {
        match ob.extract::<Option<(bool, Option<u64>)>>()? {
            None => Ok(Self::NotAllowed),
            Some((at_start_of_line, continuation)) => Ok(Self::CanRequest {
                at_start_of_line,
                continuation,
            }),
        }
    }
}

crate::impl_dynamic_native!(WrapConstraints);
