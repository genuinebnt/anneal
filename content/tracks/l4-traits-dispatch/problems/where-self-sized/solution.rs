pub trait Shape: DynShape {
    fn area(&self) -> f64;
    fn name(&self) -> String;

    /// A copy scaled by `k` (every length times k).
    fn scaled(&self, k: f64) -> Self
    where
        Self: Sized;
}

/// The dyn-compatible half, written once for every `Clone` shape by the blanket impl below.
pub trait DynShape {
    fn clone_box(&self) -> Box<dyn Shape>;
    fn scaled_box(&self, k: f64) -> Box<dyn Shape>;
}

impl<T: Shape + Clone + 'static> DynShape for T {
    fn clone_box(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }

    fn scaled_box(&self, k: f64) -> Box<dyn Shape> {
        Box::new(self.scaled(k))
    }
}

impl Clone for Box<dyn Shape> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Circle {
    pub r: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rect {
    pub w: f64,
    pub h: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.r * self.r
    }

    fn name(&self) -> String {
        format!("circle({})", self.r)
    }

    fn scaled(&self, k: f64) -> Self {
        Circle { r: self.r * k }
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }

    fn name(&self) -> String {
        format!("rect({}x{})", self.w, self.h)
    }

    fn scaled(&self, k: f64) -> Self {
        Rect {
            w: self.w * k,
            h: self.h * k,
        }
    }
}

/// A copy of the scene that later edits to `scene` don't affect.
pub fn snapshot(scene: &Vec<Box<dyn Shape>>) -> Vec<Box<dyn Shape>> {
    scene.clone()
}

/// Every shape scaled by `k`.
pub fn scale_all(scene: &[Box<dyn Shape>], k: f64) -> Vec<Box<dyn Shape>> {
    scene.iter().map(|s| s.scaled_box(k)).collect()
}
