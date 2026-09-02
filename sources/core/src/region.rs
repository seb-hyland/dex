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

    /// The centre of the region.
    pub fn center(self) -> ScreenPos {
        ScreenPos {
            x: (self.min.x + self.max.x) * 0.5,
            y: (self.min.y + self.max.y) * 0.5,
        }
    }

    /// Where a line from `from` to this region's centre first meets its edge.
    pub fn edge_towards(self, from: ScreenPos) -> ScreenPos {
        let center = self.center();
        if self.contains(from) {
            return center;
        }
        let (dx, dy) = (center.x - from.x, center.y - from.y);

        // How far along `from -> center` the region's near edge sits, per axis.
        // An axis the line does not move along cannot bound the crossing.
        let entry = |d: f32, lo: f32, hi: f32, start: f32| {
            if d == 0.0 {
                return f32::NEG_INFINITY;
            }
            let (t_lo, t_hi) = ((lo - start) / d, (hi - start) / d);
            t_lo.min(t_hi)
        };
        let t = entry(dx, self.min.x, self.max.x, from.x)
            .max(entry(dy, self.min.y, self.max.y, from.y))
            .clamp(0.0, 1.0);

        ScreenPos {
            x: from.x + dx * t,
            y: from.y + dy * t,
        }
    }

    pub fn expand(self, margin: f32) -> Self {
        ScreenRegion::from_min_max(
            ScreenPos {
                x: self.min.x - margin,
                y: self.min.y - margin,
            },
            ScreenPos {
                x: self.max.x + margin,
                y: self.max.y + margin,
            },
        )
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

impl Vector {
    /// The zero vector: no offset, and the size of nothing.
    pub const ZERO: Self = Self::new(0.0, 0.0);
}

impl Default for Vector {
    fn default() -> Self {
        Self::ZERO
    }
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
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn splat(s: f32) -> Self {
        Self::new(s, s)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn region(min: (f32, f32), max: (f32, f32)) -> ScreenRegion {
        ScreenRegion::from_min_max(
            ScreenPos { x: min.0, y: min.1 },
            ScreenPos { x: max.0, y: max.1 },
        )
    }

    fn at(x: f32, y: f32) -> ScreenPos {
        ScreenPos { x, y }
    }

    #[track_caller]
    fn assert_at(got: ScreenPos, want: ScreenPos) {
        assert!(
            (got.x - want.x).abs() < 1e-3 && (got.y - want.y).abs() < 1e-3,
            "got ({}, {}), want ({}, {})",
            got.x,
            got.y,
            want.x,
            want.y
        );
    }

    /// A wire coming straight at a face stops on that face, not in the middle.
    #[test]
    fn edge_towards_stops_on_the_near_face() {
        let r = region((100.0, 100.0), (200.0, 200.0));
        assert_at(r.edge_towards(at(0.0, 150.0)), at(100.0, 150.0));
        assert_at(r.edge_towards(at(150.0, 0.0)), at(150.0, 100.0));
        assert_at(r.edge_towards(at(400.0, 150.0)), at(200.0, 150.0));
        assert_at(r.edge_towards(at(150.0, 900.0)), at(150.0, 200.0));
    }

    /// Coming in diagonally, the first edge crossed is the one that bounds.
    #[test]
    fn edge_towards_takes_the_first_edge_crossed() {
        let r = region((100.0, 100.0), (200.0, 200.0));
        // 45 degrees from the top-left: meets the corner exactly.
        assert_at(r.edge_towards(at(50.0, 50.0)), at(100.0, 100.0));
        // A shallow approach from far left: the left edge bounds, not the top.
        let hit = r.edge_towards(at(-350.0, 100.0));
        assert!((hit.x - 100.0).abs() < 1e-3, "left edge, got x {}", hit.x);
        assert!(hit.y > 100.0 && hit.y < 150.0, "got y {}", hit.y);
    }

    /// A point already inside has no edge to stop at.
    #[test]
    fn edge_towards_from_inside_gives_the_centre() {
        let r = region((100.0, 100.0), (200.0, 200.0));
        assert_at(r.edge_towards(at(120.0, 180.0)), at(150.0, 150.0));
        assert_at(r.edge_towards(at(150.0, 150.0)), at(150.0, 150.0));
    }
}
