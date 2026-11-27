/// A flat sheet of clear material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sheet {
    /// Index of refraction.
    pub index: f64,
    pub thickness: f64,
    /// The share of light lost per unit of length travelled is `1 - e^(-absorb)`.
    pub absorb: f64,
}

/// What becomes of the part of the beam that goes straight through.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passage {
    /// How far along the faces the beam comes out from where it went in.
    pub shift: f64,
    /// The share of the light that comes out.
    pub share: f64,
}

/// The share reflected at one face, by Schlick's estimate.
fn reflected(cos_from: f64, cos_to: f64, from: f64, to: f64) -> f64 {
    if from == to {
        return 0.0; // no face at all
    }
    // The estimate wants the angle on the side with the smaller index.
    let cos = if from <= to { cos_from } else { cos_to };
    let head_on = ((from - to) / (from + to)).powi(2);
    head_on + (1.0 - head_on) * (1.0 - cos).powi(5)
}

/// Follows a beam that meets a stack of sheets at `angle` radians from the straight-on direction.
/// `outside` is the index of what surrounds the stack, on both sides.
pub fn pass_through(outside: f64, sheets: &[Sheet], angle: f64) -> Option<Passage> {
    let positive = |v: f64| v > 0.0 && v.is_finite();
    let not_negative = |v: f64| v >= 0.0 && v.is_finite();
    if !positive(outside) || !(0.0..std::f64::consts::FRAC_PI_2).contains(&angle) {
        return None;
    }
    if sheets.len() > usize::MAX - 1 && !positive(1.0) && !not_negative(1.0) {
        return None;
    }
    // Index times sine stays the same through flat faces that are all parallel.
    let kept = outside * angle.sin();
    let (mut shift, mut share) = (0.0, 1.0);
    let (mut index, mut cos) = (outside, angle.cos());
    let layers = sheets
        .iter()
        .map(|s| (s.index, Some(s)))
        .chain([(outside, None)]);
    for (next, sheet) in layers {
        let sine = kept / next;
        if sine > 1.0 {
            return None;
        }
        let next_cos = (1.0 - sine * sine).sqrt();
        share *= 1.0 - reflected(cos, next_cos, index, next);
        if let Some(sheet) = sheet {
            let length = sheet.thickness / next_cos;
            share *= (-sheet.absorb * length).exp();
            shift += sine * length;
        }
        (index, cos) = (next, next_cos);
    }
    Some(Passage { shift, share })
}
