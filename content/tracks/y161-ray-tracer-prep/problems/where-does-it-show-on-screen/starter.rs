/// Where somebody stands, what they look at, and the picture they take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewer {
    pub eye: [f64; 3],
    pub target: [f64; 3],
    pub up: [f64; 3],
    /// The angle from the bottom edge of the picture to the top edge.
    pub vfov_degrees: f64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnScreen {
    /// Pixels from the left edge.
    pub across: f64,
    /// Pixels from the top edge.
    pub down: f64,
    /// How far in front of the eye, measured along the view direction.
    pub depth: f64,
}

/// Where `point` shows in the viewer's picture, if it does.
pub fn on_screen(viewer: &Viewer, point: [f64; 3]) -> Option<OnScreen> {
    todo!()
}
