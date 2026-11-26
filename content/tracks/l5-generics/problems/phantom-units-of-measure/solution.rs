use std::fmt;
use std::marker::PhantomData;
use std::ops::{Add, Div, Mul};

pub trait Unit {
    /// How many metres one of this unit is.
    const METERS: f64;
    const SYMBOL: &'static str;
}

pub struct Meters;
pub struct Feet;
pub struct Kilometers;

impl Unit for Meters {
    const METERS: f64 = 1.0;
    const SYMBOL: &'static str = "m";
}

impl Unit for Feet {
    const METERS: f64 = 0.3048;
    const SYMBOL: &'static str = "ft";
}

impl Unit for Kilometers {
    const METERS: f64 = 1000.0;
    const SYMBOL: &'static str = "km";
}

// `U` is only a label. `PhantomData<fn() -> U>` records it without owning a `U`: `Length<U>` stays
// Send + Sync and covariant whatever `U` is. Derives would add `U: Clone`, `U: Debug`, ... bounds, so the
// traits are implemented by hand, with no bounds on `U`.
pub struct Length<U> {
    value: f64,
    unit: PhantomData<fn() -> U>,
}

impl<U> Clone for Length<U> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<U> Copy for Length<U> {}

impl<U> PartialEq for Length<U> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<U> PartialOrd for Length<U> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<U: Unit> fmt::Debug for Length<U> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.value, U::SYMBOL)
    }
}

impl<U: Unit> Length<U> {
    pub fn new(value: f64) -> Self {
        Length {
            value,
            unit: PhantomData,
        }
    }

    pub fn value(self) -> f64 {
        self.value
    }

    /// The same length in another unit.
    pub fn to<V: Unit>(self) -> Length<V> {
        Length::new(self.value * U::METERS / V::METERS)
    }
}

/// Only lengths in the same unit add up; convert first with `to`.
impl<U> Add for Length<U> {
    type Output = Length<U>;
    fn add(self, rhs: Length<U>) -> Length<U> {
        Length {
            value: self.value + rhs.value,
            unit: PhantomData,
        }
    }
}

impl<U> Mul<f64> for Length<U> {
    type Output = Length<U>;
    fn mul(self, k: f64) -> Length<U> {
        Length {
            value: self.value * k,
            unit: PhantomData,
        }
    }
}

/// How many times `rhs` fits into `self`.
impl<U> Div for Length<U> {
    type Output = f64;
    fn div(self, rhs: Length<U>) -> f64 {
        self.value / rhs.value
    }
}

impl<U: Unit> fmt::Display for Length<U> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.value, U::SYMBOL)
    }
}
