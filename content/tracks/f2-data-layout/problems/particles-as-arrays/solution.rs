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

/// What only I/O and diagnostics read.
#[derive(Clone, Copy)]
struct Cold {
    id: u64,
    charge: f32,
    radius: f32,
    temperature: f32,
    species: u32,
    flags: u32,
    tag: [u8; 32],
}

/// Structure of arrays: one dense `f32` array per hot field, so each kernel streams only the bytes it
/// uses and the compiler can process 4 or 8 particles per SIMD instruction.
pub struct Particles {
    x: Vec<f32>,
    y: Vec<f32>,
    z: Vec<f32>,
    vx: Vec<f32>,
    vy: Vec<f32>,
    vz: Vec<f32>,
    mass: Vec<f32>,
    cold: Vec<Cold>,
}

impl Particles {
    pub fn with_capacity(n: usize) -> Particles {
        let v = || Vec::with_capacity(n);
        Particles {
            x: v(),
            y: v(),
            z: v(),
            vx: v(),
            vy: v(),
            vz: v(),
            mass: v(),
            cold: Vec::with_capacity(n),
        }
    }

    pub fn push(&mut self, p: Particle) {
        self.x.push(p.pos[0]);
        self.y.push(p.pos[1]);
        self.z.push(p.pos[2]);
        self.vx.push(p.vel[0]);
        self.vy.push(p.vel[1]);
        self.vz.push(p.vel[2]);
        self.mass.push(p.mass);
        self.cold.push(Cold {
            id: p.id,
            charge: p.charge,
            radius: p.radius,
            temperature: p.temperature,
            species: p.species,
            flags: p.flags,
            tag: p.tag,
        });
    }

    pub fn len(&self) -> usize {
        self.x.len()
    }

    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Particle `i`, gathered from the arrays; panics if `i >= len()`.
    pub fn get(&self, i: usize) -> Particle {
        let c = self.cold[i];
        Particle {
            id: c.id,
            pos: [self.x[i], self.y[i], self.z[i]],
            vel: [self.vx[i], self.vy[i], self.vz[i]],
            mass: self.mass[i],
            charge: c.charge,
            radius: c.radius,
            temperature: c.temperature,
            species: c.species,
            flags: c.flags,
            tag: c.tag,
        }
    }

    /// One explicit Euler step: `vel.z -= G * dt`, then `pos += vel * dt` (with the new velocity), in one
    /// pass over six dense arrays, which the compiler vectorizes.
    pub fn step(&mut self, dt: f32) {
        let g = G * dt;
        // Re-slicing every array to the same length lets LLVM drop the bounds checks and vectorize.
        let n = self.x.len();
        let (x, y, z) = (&mut self.x[..n], &mut self.y[..n], &mut self.z[..n]);
        let (vx, vy, vz) = (&self.vx[..n], &self.vy[..n], &mut self.vz[..n]);
        for i in 0..n {
            vz[i] -= g;
            x[i] += vx[i] * dt;
            y[i] += vy[i] * dt;
            z[i] += vz[i] * dt;
        }
    }

    /// How many particles are below the ground plane: `pos.z < ground`. Reads 4 bytes a particle.
    pub fn count_below(&self, ground: f32) -> usize {
        self.z.iter().filter(|&&z| z < ground).count()
    }

    /// The total kinetic energy, Σ ½·m·|v|², summed in `f64`.
    pub fn kinetic_energy(&self) -> f64 {
        (0..self.len())
            .map(|i| {
                0.5 * self.mass[i] as f64
                    * [self.vx[i], self.vy[i], self.vz[i]]
                        .iter()
                        .map(|&v| v as f64 * v as f64)
                        .sum::<f64>()
            })
            .sum()
    }
}
