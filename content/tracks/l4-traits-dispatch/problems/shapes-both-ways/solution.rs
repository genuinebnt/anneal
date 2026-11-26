use std::f64::consts::PI;

pub trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> String;
}

pub struct Circle {
    pub radius: f64,
}

pub struct Rectangle {
    pub width: f64,
    pub height: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        PI * self.radius * self.radius
    }

    fn name(&self) -> String {
        format!("circle({})", self.radius)
    }
}

impl Shape for Rectangle {
    fn area(&self) -> f64 {
        self.width * self.height
    }

    fn name(&self) -> String {
        format!("rect({}x{})", self.width, self.height)
    }
}

impl<S: Shape + ?Sized> Shape for Box<S> {
    fn area(&self) -> f64 {
        (**self).area()
    }

    fn name(&self) -> String {
        (**self).name()
    }
}

impl<S: Shape + ?Sized> Shape for &S {
    fn area(&self) -> f64 {
        (**self).area()
    }

    fn name(&self) -> String {
        (**self).name()
    }
}

/// Static dispatch: compiled once per `T`, calls resolved at compile time.
pub fn total_area_generic<T: Shape>(shapes: &[T]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

/// Dynamic dispatch: compiled once, every call goes through the vtable.
pub fn total_area_dyn(shapes: &[&dyn Shape]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

/// The shape with the largest area; the first of them on a tie.
pub fn largest<T: Shape>(shapes: &[T]) -> Option<&T> {
    shapes
        .iter()
        .reduce(|best, s| if s.area() > best.area() { s } else { best })
}
