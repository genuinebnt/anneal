pub trait Shape: Clone {
    fn area(&self) -> f64;
    fn name(&self) -> String;

    /// A copy scaled by `k` (every length times k).
    fn scaled(&self, k: f64) -> Self;
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
