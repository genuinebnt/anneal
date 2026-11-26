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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Length<U> {
    value: f64,
    unit: PhantomData<U>,
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

/// Adds two lengths.
impl<U, V> Add<Length<V>> for Length<U> {
    type Output = Length<U>;
    fn add(self, rhs: Length<V>) -> Length<U> {
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
