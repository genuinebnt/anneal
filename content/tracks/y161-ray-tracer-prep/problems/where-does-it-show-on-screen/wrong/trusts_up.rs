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
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let unit = |a: [f64; 3]| {
        let length = dot(a, a).sqrt();
        (length > 0.0 && length.is_finite()).then(|| a.map(|v| v / length))
    };
    let Viewer {
        eye,
        target,
        up,
        vfov_degrees,
        width,
        height,
    } = *viewer;
    if width == 0 || height == 0 || !(vfov_degrees > 0.0 && vfov_degrees < 180.0) {
        return None;
    }
    let forward = unit(sub(target, eye))?;
    let side = cross(forward, unit(up)?);
    if dot(side, side).is_nan() || dot(side, side) <= 1e-18 {
        return None;
    }
    let right = unit(side)?;
    let upright = unit(up)?;
    let to = sub(point, eye);
    let depth = dot(to, forward);
    if depth.is_nan() || depth <= 1e-9 {
        return None;
    }
    // Half the picture's height one unit ahead.
    let half = (vfov_degrees.to_radians() / 2.0).tan();
    let half_wide = half * f64::from(width) / f64::from(height);
    let across = (dot(to, right) / depth / half_wide + 1.0) / 2.0 * f64::from(width);
    let down = (1.0 - dot(to, upright) / depth / half) / 2.0 * f64::from(height);
    let inside =
        (0.0..=f64::from(width)).contains(&across) && (0.0..=f64::from(height)).contains(&down);
    inside.then_some(OnScreen {
        across,
        down,
        depth,
    })
}
