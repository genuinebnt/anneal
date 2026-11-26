//! RECIPE 253 (graphics): a complete software rasterizer: transforms, near-plane clipping, edge functions, z-buffer, shading, textures.
//! LEVEL: advanced
//! Use in: understanding what a GPU does; software fallbacks, CPU previews, tools that need deterministic pixels.   Run: cargo run --release --example software_rasterizer
#![allow(dead_code)]
//!
//! PIPELINE: model -> world -> view -> projection gives CLIP space (x,y,z,w). Anything with z < -w is behind the near
//! plane, so each triangle is clipped against that plane with SUTHERLAND-HODGMAN IN CLIP SPACE (before dividing by w,
//! because w can be zero or negative there). Then the PERSPECTIVE DIVIDE (x/w, y/w, z/w) gives NDC, and the VIEWPORT
//! transform maps NDC to pixels (y flipped). Vertices are snapped to 1/256 pixel integers so everything below is exact.
//!
//! RASTERISATION: the EDGE FUNCTION E(a,b,p) = (b-a) x (p-a) is positive on one side of edge a->b. A pixel centre is
//! inside when all three edge functions are >= 0, and E_i / area are the BARYCENTRIC weights (they sum to exactly 1).
//! TOP-LEFT FILL RULE: a pixel centre exactly ON an edge is drawn only if that edge is a top or left edge. For two
//! triangles sharing an edge, the edge is top/left for exactly one of them (directions are opposite), so every pixel is
//! covered exactly once: no gaps, no double blending. The test below proves it on 5000 random quads.
//!
//! DEPTH: z/w is affine in screen space, so it is interpolated linearly and compared against the Z-BUFFER (smaller wins).
//! PERSPECTIVE-CORRECT attributes: u/w and 1/w are affine in screen space, u itself is not. So interpolate u/w and 1/w
//! and divide: u = (sum l_i u_i/w_i) / (sum l_i / w_i). Plain affine interpolation makes textures swim and bend.
//! SHADING: flat (one colour per triangle), Gouraud (light at vertices, interpolate colour), Phong (interpolate the
//! normal, light every pixel, so highlights appear inside triangles). Textures: nearest or bilinear, repeat or clamp.
//! COMMON USES: games and engines (the same stages run on GPUs), CAD/CAE viewers, film previs, GIS terrain viewers,
//! medical-imaging mesh viewers, procedural-generation previews, software renderers such as the tinyrenderer tutorial.
//! Read: https://github.com/ssloy/tinyrenderer/wiki and https://www.redblobgames.com/

// ---------- tiny linear algebra ----------
type V3 = [f32; 3];
type M4 = [[f32; 4]; 4]; // row-major, column vectors: p' = M * p

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-12 { mul(a, 1.0 / l) } else { a }
}
fn identity() -> M4 {
    let mut m = [[0.0; 4]; 4];
    for i in 0..4 {
        m[i][i] = 1.0;
    }
    m
}
fn mat_mul(a: &M4, b: &M4) -> M4 {
    let mut m = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                m[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    m
}
fn mat_vec(m: &M4, v: [f32; 4]) -> [f32; 4] {
    let mut r = [0.0; 4];
    for i in 0..4 {
        for k in 0..4 {
            r[i] += m[i][k] * v[k];
        }
    }
    r
}
fn translate(t: V3) -> M4 {
    let mut m = identity();
    for i in 0..3 {
        m[i][3] = t[i];
    }
    m
}
fn scale(s: f32) -> M4 {
    let mut m = identity();
    for i in 0..3 {
        m[i][i] = s;
    }
    m
}
fn rot_y(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[0][0] = c;
    m[0][2] = s;
    m[2][0] = -s;
    m[2][2] = c;
    m
}
fn rot_x(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    m[1][1] = c;
    m[1][2] = -s;
    m[2][1] = s;
    m[2][2] = c;
    m
}
fn look_at(eye: V3, target: V3, up: V3) -> M4 {
    let f = norm(sub(target, eye));
    let s = norm(cross(f, up));
    let u = cross(s, f);
    [
        [s[0], s[1], s[2], -dot(s, eye)],
        [u[0], u[1], u[2], -dot(u, eye)],
        [-f[0], -f[1], -f[2], dot(f, eye)],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
/// OpenGL-style: camera looks down -z, NDC z in [-1, 1], clip.w = -z_view.
fn perspective(fovy: f32, aspect: f32, n: f32, f: f32) -> M4 {
    let t = 1.0 / (fovy / 2.0).tan();
    [
        [t / aspect, 0.0, 0.0, 0.0],
        [0.0, t, 0.0, 0.0],
        [0.0, 0.0, (f + n) / (n - f), 2.0 * f * n / (n - f)],
        [0.0, 0.0, -1.0, 0.0],
    ]
}

// ---------- exact integer rasterisation core ----------
const SUB: i64 = 256; // sub-pixel precision: coordinates are integers in 1/256 pixel
type P = (i64, i64);

fn snap(x: f32) -> i64 {
    (x as f64 * SUB as f64).round() as i64
}
/// Edge function: twice the signed area of (a, b, p). Exact in i64.
fn edge(a: P, b: P, p: P) -> i64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}
/// With our sign convention (inside means all edge functions >= 0), top edges are horizontal going +x
/// and left edges go up the screen (dy < 0).
fn top_left(a: P, b: P) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    dy < 0 || (dy == 0 && dx > 0)
}
fn area2(v: [P; 3]) -> i64 {
    edge(v[0], v[1], v[2])
}

/// Calls `f(x, y, [e0,e1,e2], area)` for each covered pixel. Requires area > 0 (orient the triangle first).
/// e_i is the edge function opposite vertex i, so weight_i = e_i / area.
fn raster(v: [P; 3], w: i32, h: i32, mut f: impl FnMut(i32, i32, [i64; 3], i64)) {
    let area = area2(v);
    assert!(area > 0, "raster needs positively oriented triangles");
    let tl = [
        top_left(v[1], v[2]),
        top_left(v[2], v[0]),
        top_left(v[0], v[1]),
    ];
    let minx = (v.iter().map(|p| p.0).min().unwrap().div_euclid(SUB) as i32).max(0);
    let maxx = (v.iter().map(|p| p.0).max().unwrap().div_euclid(SUB) as i32).min(w - 1);
    let miny = (v.iter().map(|p| p.1).min().unwrap().div_euclid(SUB) as i32).max(0);
    let maxy = (v.iter().map(|p| p.1).max().unwrap().div_euclid(SUB) as i32).min(h - 1);
    for y in miny..=maxy {
        for x in minx..=maxx {
            let p = (x as i64 * SUB + SUB / 2, y as i64 * SUB + SUB / 2); // pixel centre
            let e = [
                edge(v[1], v[2], p),
                edge(v[2], v[0], p),
                edge(v[0], v[1], p),
            ];
            if (0..3).all(|i| e[i] > 0 || (e[i] == 0 && tl[i])) {
                f(x, y, e, area);
            }
        }
    }
}

/// Coverage counts (how many of the given triangles cover each pixel).
fn coverage(tris: &[[P; 3]], w: i32, h: i32) -> Vec<u8> {
    let mut c = vec![0u8; (w * h) as usize];
    for t in tris {
        let mut t = *t;
        let a = area2(t);
        if a == 0 {
            continue;
        }
        if a < 0 {
            t.swap(1, 2);
        }
        raster(t, w, h, |x, y, _, _| c[(y * w + x) as usize] += 1);
    }
    c
}

/// Bresenham: integer-only line, all octants.
fn bresenham(mut x0: i32, mut y0: i32, x1: i32, y1: i32, mut plot: impl FnMut(i32, i32)) {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        plot(x0, y0);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

// ---------- textures ----------
#[derive(Clone, Copy, PartialEq)]
enum Wrap {
    Repeat,
    Clamp,
}
#[derive(Clone, Copy, PartialEq)]
enum Filter {
    Nearest,
    Bilinear,
}
struct Texture {
    w: usize,
    h: usize,
    data: Vec<V3>,
}
impl Texture {
    fn checker(size: usize, cells: usize, a: V3, b: V3) -> Texture {
        let mut data = Vec::with_capacity(size * size);
        for y in 0..size {
            for x in 0..size {
                data.push(if (x * cells / size + y * cells / size) % 2 == 0 {
                    a
                } else {
                    b
                });
            }
        }
        Texture {
            w: size,
            h: size,
            data,
        }
    }
    fn texel(&self, x: i64, y: i64, wrap: Wrap) -> V3 {
        let (w, h) = (self.w as i64, self.h as i64);
        let (x, y) = match wrap {
            Wrap::Repeat => (x.rem_euclid(w), y.rem_euclid(h)),
            Wrap::Clamp => (x.clamp(0, w - 1), y.clamp(0, h - 1)),
        };
        self.data[(y * w + x) as usize]
    }
    fn sample(&self, u: f32, v: f32, filter: Filter, wrap: Wrap) -> V3 {
        let (fx, fy) = (u * self.w as f32, v * self.h as f32);
        match filter {
            Filter::Nearest => self.texel(fx.floor() as i64, fy.floor() as i64, wrap),
            Filter::Bilinear => {
                let (fx, fy) = (fx - 0.5, fy - 0.5); // texel centres sit at +0.5
                let (x0, y0) = (fx.floor(), fy.floor());
                let (tx, ty) = (fx - x0, fy - y0);
                let (x0, y0) = (x0 as i64, y0 as i64);
                let top = add(
                    mul(self.texel(x0, y0, wrap), 1.0 - tx),
                    mul(self.texel(x0 + 1, y0, wrap), tx),
                );
                let bot = add(
                    mul(self.texel(x0, y0 + 1, wrap), 1.0 - tx),
                    mul(self.texel(x0 + 1, y0 + 1, wrap), tx),
                );
                add(mul(top, 1.0 - ty), mul(bot, ty))
            }
        }
    }
}

// ---------- the pipeline ----------
#[derive(Clone, Copy)]
struct Vertex {
    pos: V3,
    nrm: V3,
    uv: [f32; 2],
}
#[derive(Clone, Copy, PartialEq)]
enum Shading {
    Flat,
    Gouraud,
    Phong,
}
struct Material<'a> {
    base: V3,
    tex: Option<&'a Texture>,
    filter: Filter,
    wrap: Wrap,
    shading: Shading,
}
#[derive(Clone, Copy)]
struct Settings {
    cull: bool,
    perspective: bool,
    wire: bool,
}
struct Camera {
    view: M4,
    proj: M4,
    eye: V3,
}

const LIGHT: V3 = [0.5, 0.8, 0.6];
const NA: usize = 10; // attrs: world pos 0..3, normal 3..6, uv 6..8, diffuse+ambient 8, specular 9

/// (ambient + diffuse, specular) for a surface point.
fn light(n: V3, pos: V3, eye: V3) -> (f32, f32) {
    let l = norm(LIGHT);
    let d = dot(n, l).max(0.0);
    let hv = norm(add(l, norm(sub(eye, pos))));
    let s = if d > 0.0 {
        dot(n, hv).max(0.0).powf(40.0) * 0.6
    } else {
        0.0
    };
    (0.18 + 0.82 * d, s)
}

/// A vertex in clip space plus its attributes.
#[derive(Clone, Copy)]
struct CV {
    clip: [f32; 4],
    a: [f32; NA],
}
fn lerp_cv(p: &CV, q: &CV, t: f32) -> CV {
    let mut r = *p;
    for i in 0..4 {
        r.clip[i] = p.clip[i] + (q.clip[i] - p.clip[i]) * t;
    }
    for i in 0..NA {
        r.a[i] = p.a[i] + (q.a[i] - p.a[i]) * t;
    }
    r
}
/// Sutherland-Hodgman against the near plane z >= -w (signed distance d = z + w).
fn clip_near(poly: &[CV]) -> Vec<CV> {
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (cur, nxt) = (&poly[i], &poly[(i + 1) % poly.len()]);
        let (dc, dn) = (cur.clip[2] + cur.clip[3], nxt.clip[2] + nxt.clip[3]);
        if dc >= 0.0 {
            out.push(*cur);
        }
        if (dc >= 0.0) != (dn >= 0.0) {
            out.push(lerp_cv(cur, nxt, dc / (dc - dn)));
        }
    }
    out
}

/// A vertex after the perspective divide and viewport transform.
#[derive(Clone, Copy)]
struct SV {
    p: P,
    z: f32,
    invw: f32,
    a: [f32; NA],
}

struct Frame {
    w: usize,
    h: usize,
    color: Vec<V3>,
    depth: Vec<f32>,
    wires: Vec<(P, P, f32, f32)>,
    frags: usize, // fragments that passed coverage (before depth test)
}
impl Frame {
    fn new(w: usize, h: usize, bg: V3) -> Frame {
        Frame {
            w,
            h,
            color: vec![bg; w * h],
            depth: vec![f32::INFINITY; w * h],
            wires: vec![],
            frags: 0,
        }
    }

    fn draw(&mut self, cam: &Camera, model: &M4, tri: [Vertex; 3], mat: &Material, set: Settings) {
        let vp = mat_mul(&cam.proj, &cam.view);
        let wp: Vec<V3> = tri
            .iter()
            .map(|v| {
                let r = mat_vec(model, [v.pos[0], v.pos[1], v.pos[2], 1.0]);
                [r[0], r[1], r[2]]
            })
            .collect();
        let wn: Vec<V3> = tri
            .iter()
            .map(|v| {
                let r = mat_vec(model, [v.nrm[0], v.nrm[1], v.nrm[2], 0.0]);
                norm([r[0], r[1], r[2]])
            })
            .collect();
        let face_n = norm(cross(sub(wp[1], wp[0]), sub(wp[2], wp[0])));
        let centroid = mul(add(add(wp[0], wp[1]), wp[2]), 1.0 / 3.0);
        let flat = light(face_n, centroid, cam.eye);
        let mut cvs = Vec::new();
        for i in 0..3 {
            let (d, s) = match mat.shading {
                Shading::Flat => flat,
                _ => light(wn[i], wp[i], cam.eye),
            };
            let c = mat_vec(&vp, [wp[i][0], wp[i][1], wp[i][2], 1.0]);
            let a = [
                wp[i][0],
                wp[i][1],
                wp[i][2],
                wn[i][0],
                wn[i][1],
                wn[i][2],
                tri[i].uv[0],
                tri[i].uv[1],
                d,
                s,
            ];
            cvs.push(CV { clip: c, a });
        }
        let poly = clip_near(&cvs);
        if poly.len() < 3 {
            return;
        }
        let (fw, fh) = (self.w as f32, self.h as f32);
        let sv: Vec<SV> = poly
            .iter()
            .map(|c| {
                let iw = 1.0 / c.clip[3];
                let (nx, ny, nz) = (c.clip[0] * iw, c.clip[1] * iw, c.clip[2] * iw);
                SV {
                    p: (
                        snap((nx * 0.5 + 0.5) * fw),
                        snap((1.0 - (ny * 0.5 + 0.5)) * fh),
                    ),
                    z: nz,
                    invw: iw,
                    a: c.a,
                }
            })
            .collect();
        // screen-space orientation: front faces (CCW in NDC) have NEGATIVE area because y is flipped
        let a0 = area2([sv[0].p, sv[1].p, sv[2].p]);
        if set.cull && a0 > 0 {
            return;
        }
        if set.wire {
            for i in 0..sv.len() {
                let (p, q) = (&sv[i], &sv[(i + 1) % sv.len()]);
                self.wires.push((p.p, q.p, p.z, q.z));
            }
        }
        for i in 1..sv.len() - 1 {
            let mut t = [sv[0], sv[i], sv[i + 1]];
            let a = area2([t[0].p, t[1].p, t[2].p]);
            if a == 0 {
                continue;
            }
            if a < 0 {
                t.swap(1, 2);
            }
            self.fill(&t, cam, mat, set);
        }
    }

    fn fill(&mut self, t: &[SV; 3], cam: &Camera, mat: &Material, set: Settings) {
        let (w, h) = (self.w as i32, self.h as i32);
        let Frame {
            color,
            depth,
            frags,
            ..
        } = self;
        raster([t[0].p, t[1].p, t[2].p], w, h, |x, y, e, area| {
            *frags += 1;
            let l: [f32; 3] = [0, 1, 2].map(|i| (e[i] as f64 / area as f64) as f32);
            let z = l[0] * t[0].z + l[1] * t[1].z + l[2] * t[2].z;
            let idx = (y * w + x) as usize;
            if !(z < depth[idx]) || z > 1.0 {
                return;
            }
            depth[idx] = z;
            let mut a = [0.0f32; NA];
            if set.perspective {
                let ws: [f32; 3] = [0, 1, 2].map(|i| l[i] * t[i].invw);
                let inv = 1.0 / (ws[0] + ws[1] + ws[2]);
                for k in 0..NA {
                    a[k] = (ws[0] * t[0].a[k] + ws[1] * t[1].a[k] + ws[2] * t[2].a[k]) * inv;
                }
            } else {
                for k in 0..NA {
                    a[k] = l[0] * t[0].a[k] + l[1] * t[1].a[k] + l[2] * t[2].a[k];
                }
            }
            let base = match mat.tex {
                Some(tx) => tx.sample(a[6], a[7], mat.filter, mat.wrap),
                None => mat.base,
            };
            let (d, s) = if mat.shading == Shading::Phong {
                light(norm([a[3], a[4], a[5]]), [a[0], a[1], a[2]], cam.eye)
            } else {
                (a[8], a[9])
            };
            color[idx] = [base[0] * d + s, base[1] * d + s, base[2] * d + s];
        });
    }

    /// Wireframe overlay with a depth test (small bias so lines on a surface win over it).
    fn flush_wires(&mut self, col: V3) {
        let wires = std::mem::take(&mut self.wires);
        for (p, q, z0, z1) in wires {
            let (x0, y0, x1, y1) = (
                (p.0.div_euclid(SUB)) as i32,
                (p.1.div_euclid(SUB)) as i32,
                (q.0.div_euclid(SUB)) as i32,
                (q.1.div_euclid(SUB)) as i32,
            );
            let n = ((x1 - x0).abs().max((y1 - y0).abs())).max(1) as f32;
            let mut i = 0.0;
            bresenham(x0, y0, x1, y1, |x, y| {
                let z = z0 + (z1 - z0) * (i / n);
                i += 1.0;
                if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
                    let idx = y as usize * self.w + x as usize;
                    if z - 0.004 <= self.depth[idx] {
                        self.color[idx] = col;
                    }
                }
            });
        }
    }

    fn to_rgb8(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(self.w * self.h * 3);
        for c in &self.color {
            for k in 0..3 {
                o.push((c[k].clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8);
            }
        }
        o
    }
}

// ---------- meshes ----------
struct Mesh {
    verts: Vec<Vertex>,
    tris: Vec<[usize; 3]>,
}
fn cube() -> Mesh {
    // (normal, right, up) with right x up = normal, so the corners below are counter-clockwise seen from outside
    let faces: [(V3, V3, V3); 6] = [
        ([0., 0., 1.], [1., 0., 0.], [0., 1., 0.]),
        ([0., 0., -1.], [-1., 0., 0.], [0., 1., 0.]),
        ([1., 0., 0.], [0., 0., -1.], [0., 1., 0.]),
        ([-1., 0., 0.], [0., 0., 1.], [0., 1., 0.]),
        ([0., 1., 0.], [1., 0., 0.], [0., 0., -1.]),
        ([0., -1., 0.], [1., 0., 0.], [0., 0., 1.]),
    ];
    let (mut verts, mut tris) = (vec![], vec![]);
    for (n, r, u) in faces {
        let base = verts.len();
        for (sr, su, uv) in [
            (-1.0, -1.0, [0., 1.]),
            (1.0, -1.0, [1., 1.]),
            (1.0, 1.0, [1., 0.]),
            (-1.0, 1.0, [0., 0.]),
        ] {
            let p = add(add(n, mul(r, sr)), mul(u, su));
            verts.push(Vertex { pos: p, nrm: n, uv });
        }
        tris.push([base, base + 1, base + 2]);
        tris.push([base, base + 2, base + 3]);
    }
    Mesh { verts, tris }
}
fn uv_sphere(lat: usize, lon: usize) -> Mesh {
    let (mut verts, mut tris) = (vec![], vec![]);
    for i in 0..=lat {
        let th = std::f32::consts::PI * i as f32 / lat as f32;
        for j in 0..=lon {
            let ph = 2.0 * std::f32::consts::PI * j as f32 / lon as f32;
            let p = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
            verts.push(Vertex {
                pos: p,
                nrm: p,
                uv: [j as f32 / lon as f32 * 2.0, i as f32 / lat as f32],
            });
        }
    }
    let at = |i: usize, j: usize| i * (lon + 1) + j;
    for i in 0..lat {
        for j in 0..lon {
            tris.push([at(i, j), at(i, j + 1), at(i + 1, j + 1)]);
            tris.push([at(i, j), at(i + 1, j + 1), at(i + 1, j)]);
        }
    }
    Mesh { verts, tris }
}
/// Every non-degenerate triangle must wind so its geometric normal agrees with the vertex normals.
fn winding_is_outward(m: &Mesh) -> bool {
    m.tris.iter().all(|t| {
        let [a, b, c] = t.map(|i| m.verts[i].pos);
        let n = cross(sub(b, a), sub(c, a));
        let len = dot(n, n).sqrt();
        len < 1e-6 || dot(n, m.verts[t[0]].nrm) > 0.0
    })
}
fn draw_mesh(f: &mut Frame, cam: &Camera, model: &M4, m: &Mesh, mat: &Material, set: Settings) {
    for t in &m.tris {
        f.draw(
            cam,
            model,
            [m.verts[t[0]], m.verts[t[1]], m.verts[t[2]]],
            mat,
            set,
        );
    }
}

fn std_camera(w: usize, h: usize, eye: V3) -> Camera {
    Camera {
        view: look_at(eye, [0., 0., 0.], [0., 1., 0.]),
        proj: perspective(0.9, w as f32 / h as f32, 0.1, 100.0),
        eye,
    }
}
const SET: Settings = Settings {
    cull: true,
    perspective: true,
    wire: false,
};
const BG: V3 = [0.006, 0.008, 0.015];

fn blit(dst: &mut [u8], dw: usize, src: &[u8], sw: usize, sh: usize, ox: usize, oy: usize) {
    for y in 0..sh {
        let (d, s) = (((oy + y) * dw + ox) * 3, y * sw * 3);
        dst[d..d + sw * 3].copy_from_slice(&src[s..s + sw * 3]);
    }
}

fn render_plane(w: usize, h: usize, perspective: bool, tex: &Texture) -> Frame {
    let mut f = Frame::new(w, h, BG);
    let cam = Camera {
        view: look_at([0., 0.6, 0.], [0., 0.2, -10.], [0., 1., 0.]),
        proj: perspective_fn(w, h),
        eye: [0., 0.6, 0.],
    };
    let q = |x: f32, z: f32, u: f32, v: f32| Vertex {
        pos: [x, 0.0, z],
        nrm: [0., 1., 0.],
        uv: [u, v],
    };
    let (near, far) = (-1.0, -40.0);
    let vs = [
        q(-3., near, 0., 0.),
        q(3., near, 4., 0.),
        q(3., far, 4., 4.),
        q(-3., far, 0., 4.),
    ];
    let mat = Material {
        base: [1.0; 3],
        tex: Some(tex),
        filter: Filter::Nearest,
        wrap: Wrap::Repeat,
        shading: Shading::Gouraud,
    };
    let set = Settings {
        cull: false,
        perspective,
        wire: false,
    };
    f.draw(&cam, &identity(), [vs[0], vs[1], vs[2]], &mat, set);
    f.draw(&cam, &identity(), [vs[0], vs[2], vs[3]], &mat, set);
    f
}
fn perspective_fn(w: usize, h: usize) -> M4 {
    perspective(1.0, w as f32 / h as f32, 0.1, 100.0)
}

fn flat_tri(z: f32, x: f32, col: V3) -> ([Vertex; 3], Material<'static>) {
    let v = |px: f32, py: f32| Vertex {
        pos: [px + x, py, z],
        nrm: [0., 0., 1.],
        uv: [0., 0.],
    };
    (
        [v(-1., -1.), v(1., -1.), v(0., 1.2)],
        Material {
            base: col,
            tex: None,
            filter: Filter::Nearest,
            wrap: Wrap::Repeat,
            shading: Shading::Flat,
        },
    )
}

// ---------- tests ----------
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn test_fill_rule() {
    let (w, h) = (64, 64);
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let (mut tested, mut tie_pixels) = (0, 0u32);
    for i in 0..5000 {
        let (cx, cy, r) = (
            20.0 + rng.f() * 24.0,
            20.0 + rng.f() * 24.0,
            3.0 + rng.f() * 14.0,
        );
        let mut ang: Vec<f64> = (0..4).map(|_| rng.f() * std::f64::consts::TAU).collect();
        ang.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // even-numbered quads sit on a half-pixel lattice so edges pass EXACTLY through pixel centres (worst case ties)
        let grid = if i % 2 == 0 { SUB / 2 } else { 1 };
        let q: Vec<P> = ang
            .iter()
            .map(|a| {
                let (x, y) = (
                    (cx + r * a.cos()) * SUB as f64,
                    (cy + r * a.sin()) * SUB as f64,
                );
                (
                    ((x / grid as f64).round() as i64) * grid,
                    ((y / grid as f64).round() as i64) * grid,
                )
            })
            .collect();
        let convex = (0..4).all(|k| edge(q[k], q[(k + 1) % 4], q[(k + 2) % 4]) > 0);
        if !convex {
            continue;
        }
        tested += 1;
        let d1 = coverage(&[[q[0], q[1], q[2]], [q[0], q[2], q[3]]], w, h);
        let d2 = coverage(&[[q[0], q[1], q[3]], [q[1], q[2], q[3]]], w, h);
        assert!(
            d1.iter().all(|&c| c <= 1),
            "pixel drawn twice along the shared edge"
        );
        assert_eq!(
            d1, d2,
            "the two triangulations of one quad must cover identical pixels (no gaps)"
        );
        // area oracle: covered pixel count is within the perimeter of the true area
        let area_px =
            (edge(q[0], q[1], q[2]) + edge(q[0], q[2], q[3])) as f64 / 2.0 / (SUB * SUB) as f64;
        let per: f64 = (0..4)
            .map(|k| {
                let (a, b) = (q[k], q[(k + 1) % 4]);
                (((b.0 - a.0).pow(2) + (b.1 - a.1).pow(2)) as f64).sqrt() / SUB as f64
            })
            .sum();
        let n = d1.iter().filter(|&&c| c == 1).count() as f64;
        assert!(
            (n - area_px).abs() <= per + 2.0,
            "coverage {n} vs area {area_px}"
        );
        tie_pixels += (0..4)
            .map(|k| (q[k].0 % SUB == SUB / 2) as u32)
            .sum::<u32>();
        // split a triangle at an interior point into 3: must equal the whole triangle, exactly once each
        let (t0, t1, t2) = (q[0], q[1], q[2]);
        let (u, v) = (rng.f() * 0.5, rng.f() * 0.5);
        let pt = (
            t0.0 + ((t1.0 - t0.0) as f64 * u + (t2.0 - t0.0) as f64 * v) as i64,
            t0.1 + ((t1.1 - t0.1) as f64 * u + (t2.1 - t0.1) as f64 * v) as i64,
        );
        let whole = coverage(&[[t0, t1, t2]], w, h);
        let parts = coverage(&[[pt, t0, t1], [pt, t1, t2], [pt, t2, t0]], w, h);
        assert_eq!(whole, parts, "3-way split differs from the whole triangle");
    }
    assert!(tested > 3000, "too few valid quads: {tested}");
    println!(
        "fill rule: {tested} random convex quads, no gaps, no double-draws ({tie_pixels} lattice-aligned vertices)"
    );
}

fn test_barycentrics() {
    let t = [
        (10 * SUB + 37, 5 * SUB + 3),
        (60 * SUB + 200, 20 * SUB + 90),
        (25 * SUB + 5, 58 * SUB + 11),
    ];
    let mut n = 0;
    raster(t, 64, 64, |_, _, e, area| {
        assert_eq!(
            e[0] + e[1] + e[2],
            area,
            "edge functions must sum to the area exactly"
        );
        let s: f64 = e.iter().map(|&x| x as f64 / area as f64).sum();
        assert!((s - 1.0).abs() < 1e-12 && e.iter().all(|&x| x >= 0));
        n += 1;
    });
    assert!(n > 500);
    println!("barycentric weights: sum to exactly 1 on {n} pixels");
}

fn main() {
    println!("== software rasterizer ==");
    test_fill_rule();
    test_barycentrics();

    // Bresenham sanity: endpoints included, 8-connected
    let mut pts = vec![];
    bresenham(2, 3, 40, -9, |x, y| pts.push((x, y)));
    assert_eq!(pts[0], (2, 3));
    assert_eq!(*pts.last().unwrap(), (40, -9));
    assert!(
        pts.windows(2)
            .all(|w| (w[0].0 - w[1].0).abs() <= 1 && (w[0].1 - w[1].1).abs() <= 1)
    );

    // textures
    let tex = Texture::checker(64, 8, [0.95, 0.9, 0.8], [0.15, 0.25, 0.55]);
    assert_eq!(
        tex.sample(0.3, 0.2, Filter::Nearest, Wrap::Repeat),
        tex.sample(1.3, -0.8, Filter::Nearest, Wrap::Repeat)
    );
    assert_eq!(
        tex.sample(5.0, 0.5, Filter::Nearest, Wrap::Clamp),
        tex.texel(63, 32, Wrap::Clamp)
    );
    let centre = tex.sample(0.5 / 64.0, 0.5 / 64.0, Filter::Bilinear, Wrap::Repeat);
    assert_eq!(
        centre,
        tex.texel(0, 0, Wrap::Repeat),
        "bilinear at a texel centre returns the texel"
    );
    let edge_mix = tex.sample(8.0 / 64.0, 0.5 / 64.0, Filter::Bilinear, Wrap::Repeat); // exactly on a square boundary
    let (c0, c1) = (tex.texel(7, 0, Wrap::Repeat), tex.texel(8, 0, Wrap::Repeat));
    assert!(
        (edge_mix[0] - (c0[0] + c1[0]) / 2.0).abs() < 1e-5,
        "bilinear blends the two squares 50/50"
    );
    println!("textures: repeat/clamp wrap and bilinear blend verified");

    // mesh winding
    assert!(winding_is_outward(&cube()) && winding_is_outward(&uv_sphere(16, 24)));

    // depth test independent of draw order
    let cam = std_camera(64, 48, [0., 0., 4.]);
    let (near_t, near_m) = flat_tri(0.5, 0.0, [1.0, 0.1, 0.1]);
    let (far_t, far_m) = flat_tri(-1.0, 0.3, [0.1, 0.1, 1.0]);
    let render_order = |near_first: bool| {
        let mut f = Frame::new(64, 48, BG);
        let s = Settings { cull: false, ..SET };
        if near_first {
            f.draw(&cam, &identity(), near_t, &near_m, s);
            f.draw(&cam, &identity(), far_t, &far_m, s);
        } else {
            f.draw(&cam, &identity(), far_t, &far_m, s);
            f.draw(&cam, &identity(), near_t, &near_m, s);
        }
        f
    };
    let (a, b) = (render_order(true), render_order(false));
    assert_eq!(
        a.color, b.color,
        "draw order must not matter with a z-buffer"
    );
    let c = a.color[24 * 64 + 32];
    assert!(
        c[0] > c[2] * 3.0,
        "centre pixel shows the NEARER (red) triangle, got {c:?}"
    );
    println!(
        "z-buffer: nearer triangle wins in both draw orders (centre = {:.2},{:.2},{:.2})",
        c[0], c[1], c[2]
    );

    // culling a back-facing triangle; clipping fully outside
    let mut back = near_t;
    back.swap(1, 2);
    let mut f = Frame::new(64, 48, BG);
    f.draw(&cam, &identity(), back, &near_m, SET);
    assert!(f.color.iter().all(|&c| c == BG), "back face must be culled");
    f.draw(&cam, &identity(), near_t, &near_m, SET);
    assert!(f.color.iter().any(|&c| c != BG), "front face draws");
    let mut f = Frame::new(64, 48, BG);
    let behind = near_t.map(|v| Vertex {
        pos: [v.pos[0], v.pos[1], 9.0],
        ..v
    }); // behind the camera at z=4
    let aside = near_t.map(|v| Vertex {
        pos: [v.pos[0] + 50.0, v.pos[1], 0.0],
        ..v
    });
    f.draw(
        &cam,
        &identity(),
        behind,
        &near_m,
        Settings { cull: false, ..SET },
    );
    f.draw(
        &cam,
        &identity(),
        aside,
        &near_m,
        Settings { cull: false, ..SET },
    );
    assert!(
        f.color.iter().all(|&c| c == BG) && f.depth.iter().all(|d| d.is_infinite()),
        "outside the frustum draws nothing"
    );
    // a ground triangle crossing the near plane is clipped, not dropped
    let mut f = Frame::new(64, 48, BG);
    let g = |x: f32, z: f32| Vertex {
        pos: [x, -1.0, z],
        nrm: [0., 1., 0.],
        uv: [0., 0.],
    };
    f.draw(
        &cam,
        &identity(),
        [g(-5., 8.0), g(5., 8.0), g(0., -20.0)],
        &near_m,
        Settings { cull: false, ..SET },
    );
    let cov = f.color.iter().filter(|&&c| c != BG).count();
    assert!(
        cov > 100 && f.depth.iter().all(|&d| d <= 1.0 || d.is_infinite()),
        "near-plane clipping keeps the visible part ({cov} px)"
    );
    println!(
        "culling and clipping: back face culled, outside-frustum draws nothing, near-plane clip keeps {cov} px"
    );

    // affine vs perspective-correct on a receding plane
    let (pw, ph) = (200, 150);
    let (fp, fa) = (
        render_plane(pw, ph, true, &tex),
        render_plane(pw, ph, false, &tex),
    );
    let far_rows = 0..ph / 2;
    let differing = far_rows
        .clone()
        .map(|y| {
            (0..pw)
                .filter(|&x| fp.color[y * pw + x] != fa.color[y * pw + x])
                .count()
        })
        .sum::<usize>();
    let covered = far_rows
        .map(|y| (0..pw).filter(|&x| fp.color[y * pw + x] != BG).count())
        .sum::<usize>();
    assert!(
        covered > 0 && differing as f32 > 0.1 * covered as f32,
        "affine and perspective must differ at the far end: {differing}/{covered}"
    );
    println!(
        "texture warp: affine differs from perspective-correct on {differing}/{covered} far-half pixels"
    );

    // main scene: textured cube (Phong, bilinear, wireframe) + sphere
    let (w, h) = (400usize, 300usize);
    let cam = std_camera(w, h, [0., 1.2, 7.0]);
    let (cube_m, sphere_m) = (cube(), uv_sphere(20, 32));
    let tex_big = Texture::checker(64, 8, [0.95, 0.85, 0.5], [0.2, 0.3, 0.6]);
    let bil = Material {
        base: [1.0; 3],
        tex: Some(&tex_big),
        filter: Filter::Bilinear,
        wrap: Wrap::Repeat,
        shading: Shading::Phong,
    };
    let t0 = std::time::Instant::now();
    let mut spins = 0;
    for k in 0..24 {
        let mut f = Frame::new(w, h, BG);
        let m = mat_mul(
            &translate([0., 0., 0.]),
            &mat_mul(&rot_y(k as f32 * 0.26), &rot_x(0.5)),
        );
        draw_mesh(
            &mut f,
            &std_camera(w, h, [0., 0., 5.0]),
            &m,
            &cube_m,
            &bil,
            SET,
        );
        assert!(
            f.color[(h / 2) * w + w / 2] != BG,
            "cube must cover the centre pixel (frame {k})"
        );
        spins += 1;
    }
    let mut f = Frame::new(w, h, BG);
    let m1 = mat_mul(
        &translate([-1.7, 0., 0.]),
        &mat_mul(&rot_y(0.7), &rot_x(0.5)),
    );
    draw_mesh(
        &mut f,
        &cam,
        &m1,
        &cube_m,
        &bil,
        Settings { wire: true, ..SET },
    );
    f.flush_wires([0.0, 0.0, 0.0]);
    let m2 = mat_mul(&translate([1.9, 0., 0.]), &scale(1.4));
    let chk = Material {
        base: [1.0; 3],
        tex: Some(&tex_big),
        filter: Filter::Bilinear,
        wrap: Wrap::Repeat,
        shading: Shading::Phong,
    };
    draw_mesh(&mut f, &cam, &m2, &sphere_m, &chk, SET);
    println!("{spins} cube frames rendered in {:.0?}", t0.elapsed());
    assert!(
        f.color[(h / 2) * w + w / 2] != BG || f.color[(h / 2) * w + w / 2 - 60] != BG,
        "scene is not empty"
    );
    let scene = f.to_rgb8();
    save_image("r253_scene", w, h, &scene);

    // culling must not change a closed mesh's image, only the work done
    let mk = |cull: bool| {
        let mut f = Frame::new(160, 120, BG);
        draw_mesh(
            &mut f,
            &std_camera(160, 120, [0., 0., 5.]),
            &rot_y(0.6),
            &sphere_m,
            &bil,
            Settings { cull, ..SET },
        );
        f
    };
    let (c1, c0) = (mk(true), mk(false));
    assert_eq!(
        c1.color, c0.color,
        "culling closed meshes does not change the picture"
    );
    assert!(
        c1.frags < c0.frags,
        "but it saves fragment work: {} < {}",
        c1.frags,
        c0.frags
    );
    println!(
        "back-face culling: {} fragments vs {} without",
        c1.frags, c0.frags
    );

    // comparison sheet: plane affine | perspective, then sphere flat | gouraud | phong
    let mut sheet = vec![0u8; w * h * 3];
    blit(&mut sheet, w, &fa.to_rgb8(), pw, ph, 0, 0);
    blit(&mut sheet, w, &fp.to_rgb8(), pw, ph, 200, 0);
    let scam = std_camera(133, 150, [0., 0., 3.6]);
    for (i, sh) in [Shading::Flat, Shading::Gouraud, Shading::Phong]
        .into_iter()
        .enumerate()
    {
        let mut f = Frame::new(133, 150, BG);
        let m = Material {
            base: [0.9, 0.35, 0.3],
            tex: None,
            filter: Filter::Nearest,
            wrap: Wrap::Repeat,
            shading: sh,
        };
        draw_mesh(&mut f, &scam, &rot_y(0.3), &uv_sphere(8, 12), &m, SET);
        blit(&mut sheet, w, &f.to_rgb8(), 133, 150, i * 133, 150);
    }
    save_image("r253_compare", w, h, &sheet);
    println!(
        "top row: affine (bent checker) | perspective-correct; bottom: flat | Gouraud | Phong sphere"
    );
    ascii_preview(w, h, &scene, 72);
    println!("all rasterizer assertions passed");
}

// ---------- image output helpers (copied into each recipe so every file stays standalone) ----------
fn gallery_dir() -> std::path::PathBuf {
    let d = match std::env::var_os("COOKBOOK_GALLERY") {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join("flowforge_gallery"),
    };
    std::fs::create_dir_all(&d).expect("create gallery dir");
    d
}

/// Binary PPM (P6): the simplest image format there is: a text header then raw RGB bytes.
fn write_ppm(path: &std::path::Path, w: usize, h: usize, rgb: &[u8]) {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    out.extend_from_slice(rgb);
    std::fs::write(path, out).expect("write ppm");
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Minimal PNG: signature, IHDR, one IDAT holding a zlib stream of STORED (uncompressed) deflate blocks, IEND.
fn write_png(path: &std::path::Path, w: usize, h: usize, rgb: &[u8]) {
    assert_eq!(rgb.len(), w * h * 3);
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for y in 0..h {
        raw.push(0); // filter type 0 = none
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }
    let mut z = vec![0x78, 0x01]; // zlib header, no compression
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        z.push(if chunks.peek().is_none() { 1 } else { 0 }); // BFINAL, BTYPE=00 (stored)
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolour RGB
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    std::fs::write(path, png).expect("write png");
}

/// Save both PPM and PNG next to each other; prints the PNG path.
fn save_image(name: &str, w: usize, h: usize, rgb: &[u8]) {
    assert!(
        w <= 400 && h <= 300,
        "gallery images are limited to 400x300"
    );
    let dir = gallery_dir();
    write_ppm(&dir.join(format!("{name}.ppm")), w, h, rgb);
    let p = dir.join(format!("{name}.png"));
    write_png(&p, w, h, rgb);
    println!("wrote {} ({w}x{h})", p.display());
}

/// Terminal preview: average each cell's luminance and map it through a character ramp.
fn ascii_preview(w: usize, h: usize, rgb: &[u8], cols: usize) {
    let ramp = b" .:-=+*#%@";
    let cw = (w as f32 / cols as f32).max(1.0);
    let ch = cw * 2.0; // terminal cells are about twice as tall as wide
    let rows = (h as f32 / ch).floor() as usize;
    for r in 0..rows {
        let mut line = String::new();
        for c in 0..cols {
            let (x0, x1) = (
                (c as f32 * cw) as usize,
                (((c + 1) as f32 * cw) as usize).min(w),
            );
            let (y0, y1) = (
                (r as f32 * ch) as usize,
                (((r + 1) as f32 * ch) as usize).min(h),
            );
            let (mut s, mut n) = (0.0f32, 0.0f32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 3;
                    s += 0.2126 * rgb[i] as f32
                        + 0.7152 * rgb[i + 1] as f32
                        + 0.0722 * rgb[i + 2] as f32;
                    n += 1.0;
                }
            }
            let l = if n > 0.0 { s / n / 255.0 } else { 0.0 };
            line.push(ramp[((l * 9.0).round() as usize).min(9)] as char);
        }
        println!("{line}");
    }
}
