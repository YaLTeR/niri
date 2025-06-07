use std::marker::PhantomData;

use smithay::utils::{Coordinate, Logical, Point, Size};

pub mod axis {
    pub struct X;
    pub struct Y;
    pub struct U;
    pub struct V;

    pub trait Axis {}
    impl Axis for X {}
    impl Axis for Y {}
    impl Axis for U {}
    impl Axis for V {}
}

pub use axis::*;

pub struct Coord<A: Axis, N: Coordinate, Kind> {
    value: N,
    _axis: PhantomData<A>,
    _kind: PhantomData<Kind>,
}
pub type UCoord = Coord<U, f64, Logical>;
pub type VCoord = Coord<V, f64, Logical>;

impl<A: Axis, N: Coordinate, Kind> Copy for Coord<A, N, Kind> {}
impl<A: Axis, N: Coordinate, Kind> Clone for Coord<A, N, Kind> {
    #[allow(clippy::non_canonical_clone_impl)]
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            _axis: PhantomData,
            _kind: PhantomData,
        }
    }
}
impl<A: Axis, N: Coordinate, Kind> PartialEq for Coord<A, N, Kind> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}
impl<A: Axis, N: Coordinate, Kind> PartialOrd for Coord<A, N, Kind> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}
impl<A: Axis, N: Coordinate, Kind> std::fmt::Debug for Coord<A, N, Kind> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}
impl<A: Axis, N: Coordinate, Kind> Default for Coord<A, N, Kind> {
    fn default() -> Self {
        Self::zero()
    }
}

impl<A: Axis, N: Coordinate, Kind> Coord<A, N, Kind> {
    pub fn new(value: N) -> Self {
        Self {
            value,
            _axis: PhantomData,
            _kind: PhantomData,
        }
    }
    pub fn zero() -> Self {
        Self::new(N::ZERO)
    }
    pub fn get(&self) -> N {
        self.value
    }

    pub fn min(self, other: Self) -> Self {
        if other < self {
            other
        } else {
            self
        }
    }

    pub fn max(self, other: Self) -> Self {
        if self < other {
            other
        } else {
            self
        }
    }

    pub fn clamp(mut self, min: Self, max: Self) -> Self {
        if self < min {
            self = min;
        }

        if self > max {
            self = max;
        }

        self
    }

    pub fn abs(self) -> Self {
        if self.value < N::ZERO {
            Self::new(N::ZERO - self.value)
        } else {
            self
        }
    }
}

// Implement arithmetics
impl<A: Axis, N: Coordinate, R: Coordinate, O: Coordinate, Kind> std::ops::Add<Coord<A, R, Kind>>
    for Coord<A, N, Kind>
where
    N: std::ops::Add<R, Output = O>,
{
    type Output = Coord<A, O, Kind>;

    fn add(self, rhs: Coord<A, R, Kind>) -> Self::Output {
        Self::Output::new(self.value + rhs.value)
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, Kind> std::ops::AddAssign<Coord<A, R, Kind>>
    for Coord<A, N, Kind>
where
    N: std::ops::AddAssign<R>,
{
    fn add_assign(&mut self, rhs: Coord<A, R, Kind>) {
        self.value += rhs.value;
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, O: Coordinate, Kind> std::ops::Sub<Coord<A, R, Kind>>
    for Coord<A, N, Kind>
where
    N: std::ops::Sub<R, Output = O>,
{
    type Output = Coord<A, O, Kind>;

    fn sub(self, rhs: Coord<A, R, Kind>) -> Self::Output {
        Self::Output::new(self.value - rhs.value)
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, Kind> std::ops::SubAssign<Coord<A, R, Kind>>
    for Coord<A, N, Kind>
where
    N: std::ops::SubAssign<R>,
{
    fn sub_assign(&mut self, rhs: Coord<A, R, Kind>) {
        self.value -= rhs.value;
    }
}

impl<A: Axis, N: Coordinate, Kind> std::ops::Neg for Coord<A, N, Kind> {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(N::ZERO - self.value)
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, O: Coordinate, Kind> std::ops::Mul<R>
    for Coord<A, N, Kind>
where
    N: std::ops::Mul<R, Output = O>,
{
    type Output = Coord<A, O, Kind>;

    fn mul(self, rhs: R) -> Self::Output {
        Self::Output::new(self.value * rhs)
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, Kind> std::ops::MulAssign<R> for Coord<A, N, Kind>
where
    N: std::ops::MulAssign<R>,
{
    fn mul_assign(&mut self, rhs: R) {
        self.value *= rhs;
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, O: Coordinate, Kind> std::ops::Div<R>
    for Coord<A, N, Kind>
where
    N: std::ops::Div<R, Output = O>,
{
    type Output = Coord<A, O, Kind>;

    fn div(self, rhs: R) -> Self::Output {
        Self::Output::new(self.value / rhs)
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, Kind> std::ops::DivAssign<R> for Coord<A, N, Kind>
where
    N: std::ops::DivAssign<R>,
{
    fn div_assign(&mut self, rhs: R) {
        self.value /= rhs;
    }
}

impl<A: Axis, N: Coordinate, R: Coordinate, O: Coordinate, Kind> std::ops::Div<Coord<A, R, Kind>>
    for Coord<A, N, Kind>
where
    N: std::ops::Div<R, Output = O>,
{
    type Output = O;

    fn div(self, rhs: Coord<A, R, Kind>) -> Self::Output {
        self.value / rhs.value
    }
}

impl<A: Axis, N: Coordinate, Kind> std::iter::Sum for Coord<A, N, Kind>
where
    N: std::ops::AddAssign,
{
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut sum = N::ZERO;

        for i in iter {
            sum += i.value;
        }

        Self::new(sum)
    }
}

// UVPair

#[derive(Copy, Clone, PartialEq, PartialOrd, Debug)]
pub struct UVPair<N: Coordinate, K> {
    pub u: Coord<U, N, K>,
    pub v: Coord<V, N, K>,
}

impl<N: Coordinate, Kind> From<(Coord<U, N, Kind>, Coord<V, N, Kind>)> for UVPair<N, Kind> {
    fn from(value: (Coord<U, N, Kind>, Coord<V, N, Kind>)) -> Self {
        let (u, v) = value;
        Self { u, v }
    }
}

impl<N: Coordinate, Kind> std::ops::AddAssign for UVPair<N, Kind>
where
    N: std::ops::AddAssign<N>,
{
    fn add_assign(&mut self, rhs: Self) {
        self.u += rhs.u;
        self.v += rhs.v;
    }
}

impl<N: Coordinate, Kind> std::ops::SubAssign for UVPair<N, Kind>
where
    N: std::ops::SubAssign<N>,
{
    fn sub_assign(&mut self, rhs: Self) {
        self.u -= rhs.u;
        self.v -= rhs.v;
    }
}
// Orientation

#[derive(Debug, Clone, Copy)]
pub enum Orientation {
    XY,
    YX,
}

impl Orientation {
    pub fn size_to_uv<N: Coordinate, Kind>(&self, size: Size<N, Kind>) -> UVPair<N, Kind> {
        let Size { w: x, h: y, .. } = size;
        let (u, v) = match self {
            Orientation::XY => (x, y),
            Orientation::YX => (y, x),
        };
        UVPair {
            u: Coord::new(u),
            v: Coord::new(v),
        }
    }
    pub fn uv_to_size<N: Coordinate, Kind>(&self, uv: impl Into<UVPair<N, Kind>>) -> Size<N, Kind> {
        let uv = uv.into();
        let u = uv.u.get();
        let v = uv.v.get();
        let (x, y) = match self {
            Orientation::XY => (u, v),
            Orientation::YX => (v, u),
        };
        (x, y).into()
    }

    pub fn point_to_uv<N: Coordinate, Kind>(&self, point: Point<N, Kind>) -> UVPair<N, Kind> {
        let Point { x, y, .. } = point;
        let (u, v) = match self {
            Orientation::XY => (x, y),
            Orientation::YX => (y, x),
        };
        UVPair {
            u: Coord::new(u),
            v: Coord::new(v),
        }
    }
    pub fn uv_to_point<N: Coordinate, Kind>(
        &self,
        uv: impl Into<UVPair<N, Kind>>,
    ) -> Point<N, Kind> {
        let uv = uv.into();
        let u = uv.u.get();
        let v = uv.v.get();
        let (x, y) = match self {
            Orientation::XY => (u, v),
            Orientation::YX => (v, u),
        };
        (x, y).into()
    }
}
