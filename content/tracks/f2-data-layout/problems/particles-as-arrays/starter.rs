/// Gravity, in m/s², along -z.
pub const G: f32 = 9.81;

/// A particle as checkpoints and the rest of the code see it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub id: u64,
    pub pos: [f32; 3],
    pub vel: [f32; 3],
    pub mass: f32,
    pub charge: f32,
    pub radius: f32,
    pub temperature: f32,
    pub species: u32,
    pub flags: u32,
    pub tag: [u8; 32],
}

/// The simulation's particles, one `Particle` after another (88 bytes each).
pub struct Particles {
    items: Vec<Particle>,
}

impl Particles {
    pub fn with_capacity(n: usize) -> Particles {
        Particles {
            items: Vec::with_capacity(n),
        }
    }

    pub fn push(&mut self, p: Particle) {
        self.items.push(p);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Particle `i`; panics if `i >= len()`.
    pub fn get(&self, i: usize) -> Particle {
        self.items[i]
    }

    /// One explicit Euler step: `vel.z -= G * dt`, then `pos += vel * dt` (with the new velocity).
    pub fn step(&mut self, dt: f32) {
        for p in &mut self.items {
            p.vel[2] -= G * dt;
            p.pos[0] += p.vel[0] * dt;
            p.pos[1] += p.vel[1] * dt;
            p.pos[2] += p.vel[2] * dt;
        }
    }

    /// How many particles are below the ground plane: `pos.z < ground`.
    pub fn count_below(&self, ground: f32) -> usize {
        self.items.iter().filter(|p| p.pos[2] < ground).count()
    }

    /// The total kinetic energy, Σ ½·m·|v|², summed in `f64`.
    pub fn kinetic_energy(&self) -> f64 {
        self.items
            .iter()
            .map(|p| 0.5 * p.mass as f64 * p.vel.iter().map(|&v| v as f64 * v as f64).sum::<f64>())
            .sum()
    }
}
