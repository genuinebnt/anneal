//! RECIPE 254 (graphics): 2D drawing primitives: lines, circles, fills, Bezier and Catmull-Rom curves, SDFs, bitmap text.
//! LEVEL: intermediate
//! Use in: drawing libraries, plotting, UI/vector renderers, game sprites, map rendering, image tooling.   Run: cargo run --release --example lines_curves_fills
#![allow(dead_code)]
//!
//! BRESENHAM: step along the major axis; keep an integer error term of how far the pixel is from the true line; bump
//! the minor coordinate when the error crosses half a pixel. No floats, 8-connected, hits both endpoints.
//! XIAOLIN WU: same walk but plots TWO pixels per column with coverage (1-f, f) where f is the fractional line height,
//! so the two always sum to 1 and the edge looks anti-aliased. MIDPOINT circle/ellipse: walk one octant (quadrant for the
//! ellipse, in two regions where the slope passes 1) deciding between two candidate pixels by the sign of the implicit
//! function at their midpoint, then mirror. THICK lines: distance from pixel centre to the SEGMENT (a capsule SDF) <= r
//! gives round caps for free.
//! FLOOD FILL uses an explicit stack, so a 400x300 canvas cannot overflow the call stack (recursion would). 4-connected
//! fill cannot cross a diagonal 1-pixel wall; 8-connected can, so the choice matters. SCANLINE POLYGON fill: for each
//! scanline find edge crossings at y+0.5 (half-open rule so vertices count once), sort by x, then fill spans by EVEN-ODD
//! (toggle) or NON-ZERO winding (sum of +-1 directions). BEZIER via de Casteljau: repeated lerp; the same step splits
//! the curve in two, which gives ADAPTIVE flattening: subdivide until the control points are within `tol` of the chord.
//! CATMULL-ROM spline passes through its points: each segment is the Bezier with controls p1+(p2-p0)/6 and p2-(p3-p1)/6.
//! SDFs return signed distance (negative inside); smooth-min blends shapes into blobs.
//! COMMON USES: vector graphics and font rasterisers, plotting libraries, GIS map rendering, CAD sketchers, game UI and
//! sprite tools, procedural textures (SDF text and shapes), image annotation in medical-imaging viewers.
//! Read: https://www.redblobgames.com/ and https://github.com/ssloy/tinyrenderer/wiki

type Pt = (f64, f64);
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
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f()
    }
}

type Rgb = [f32; 3];
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<Rgb>,
}
impl Canvas {
    fn new(w: usize, h: usize, c: Rgb) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![c; w * h],
        }
    }
    fn inb(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }
    fn get(&self, x: i32, y: i32) -> Rgb {
        self.px[y as usize * self.w + x as usize]
    }
    fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if self.inb(x, y) {
            let w = self.w;
            self.px[y as usize * w + x as usize] = c;
        }
    }
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f64) {
        if self.inb(x, y) {
            let a = a.clamp(0.0, 1.0) as f32;
            let w = self.w;
            let p = &mut self.px[y as usize * w + x as usize];
            for k in 0..3 {
                p[k] = p[k] * (1.0 - a) + c[k] * a;
            }
        }
    }
    fn to_rgb8(&self) -> Vec<u8> {
        self.px
            .iter()
            .flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8))
            .collect()
    }
}

// ---------- lines ----------
fn bresenham(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    let mut out = vec![];
    loop {
        out.push((x, y));
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
    out
}

/// Xiaolin Wu: calls plot(x, y, coverage). Two pixels per major-axis step; the pair sums to 1.
fn wu_line(
    mut x0: f64,
    mut y0: f64,
    mut x1: f64,
    mut y1: f64,
    mut plot: impl FnMut(i32, i32, f64),
) {
    let steep = (y1 - y0).abs() > (x1 - x0).abs();
    if steep {
        std::mem::swap(&mut x0, &mut y0);
        std::mem::swap(&mut x1, &mut y1);
    }
    if x0 > x1 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }
    let grad = if x1 > x0 { (y1 - y0) / (x1 - x0) } else { 0.0 };
    for x in x0.round() as i32..=x1.round() as i32 {
        let y = y0 + grad * (x as f64 - x0);
        let (fy, f) = (y.floor(), y - y.floor());
        for (yy, cov) in [(fy as i32, 1.0 - f), (fy as i32 + 1, f)] {
            if steep {
                plot(yy, x, cov)
            } else {
                plot(x, yy, cov)
            }
        }
    }
}

fn dist_seg(p: Pt, a: Pt, b: Pt) -> f64 {
    let (ab, ap) = ((b.0 - a.0, b.1 - a.1), (p.0 - a.0, p.1 - a.1));
    let l2 = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if l2 > 0.0 {
        ((ap.0 * ab.0 + ap.1 * ab.1) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((ap.0 - t * ab.0).powi(2) + (ap.1 - t * ab.1).powi(2)).sqrt()
}

/// Thick line with round caps: coverage from the capsule SDF, 1-pixel soft edge.
fn thick_line(c: &mut Canvas, a: Pt, b: Pt, r: f64, col: Rgb) {
    let (x0, x1) = (
        (a.0.min(b.0) - r - 2.0).floor() as i32,
        (a.0.max(b.0) + r + 2.0).ceil() as i32,
    );
    let (y0, y1) = (
        (a.1.min(b.1) - r - 2.0).floor() as i32,
        (a.1.max(b.1) + r + 2.0).ceil() as i32,
    );
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = dist_seg((x as f64 + 0.5, y as f64 + 0.5), a, b);
            let cov = r + 0.5 - d;
            if cov > 0.0 {
                c.blend(x, y, col, cov);
            }
        }
    }
}

// ---------- circles and ellipses ----------
fn circle_points(cx: i32, cy: i32, r: i32) -> Vec<(i32, i32)> {
    let mut out = vec![];
    let (mut x, mut y, mut d) = (0, r, 1 - r);
    while x <= y {
        for (px, py) in [
            (x, y),
            (y, x),
            (-x, y),
            (-y, x),
            (x, -y),
            (y, -x),
            (-x, -y),
            (-y, -x),
        ] {
            out.push((cx + px, cy + py));
        }
        if d < 0 {
            d += 2 * x + 3;
        } else {
            d += 2 * (x - y) + 5;
            y -= 1;
        }
        x += 1;
    }
    out
}
fn ellipse_points(cx: i32, cy: i32, a: i32, b: i32) -> Vec<(i32, i32)> {
    let (a2, b2) = ((a * a) as f64, (b * b) as f64);
    let (mut x, mut y) = (0i32, b);
    let mut out = vec![];
    let quad = |x: i32, y: i32, out: &mut Vec<(i32, i32)>| {
        for (sx, sy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            out.push((cx + sx * x, cy + sy * y));
        }
    };
    let mut d1 = b2 - a2 * b as f64 + a2 / 4.0;
    while b2 * (x as f64) < a2 * (y as f64) {
        // region 1: slope magnitude < 1, step in x
        quad(x, y, &mut out);
        if d1 < 0.0 {
            d1 += b2 * (2 * x + 3) as f64;
        } else {
            d1 += b2 * (2 * x + 3) as f64 + a2 * (-2 * y + 2) as f64;
            y -= 1;
        }
        x += 1;
    }
    let mut d2 = b2 * (x as f64 + 0.5).powi(2) + a2 * ((y - 1) as f64).powi(2) - a2 * b2;
    while y >= 0 {
        // region 2: step in y
        quad(x, y, &mut out);
        if d2 > 0.0 {
            d2 += a2 * (-2 * y + 3) as f64;
        } else {
            d2 += b2 * (2 * x + 2) as f64 + a2 * (-2 * y + 3) as f64;
            x += 1;
        }
        y -= 1;
    }
    out
}

// ---------- fills ----------
/// Iterative flood fill with an explicit stack. Returns the number of pixels changed.
fn flood_fill(c: &mut Canvas, sx: i32, sy: i32, fill: Rgb, eight: bool) -> usize {
    if !c.inb(sx, sy) {
        return 0;
    }
    let target = c.get(sx, sy);
    if target == fill {
        return 0;
    }
    let dirs: &[(i32, i32)] = if eight {
        &[
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ]
    } else {
        &[(1, 0), (-1, 0), (0, 1), (0, -1)]
    };
    let (mut stack, mut n) = (vec![(sx, sy)], 0);
    while let Some((x, y)) = stack.pop() {
        if !c.inb(x, y) || c.get(x, y) != target {
            continue;
        }
        c.put(x, y, fill);
        n += 1;
        for (dx, dy) in dirs {
            stack.push((x + dx, y + dy));
        }
    }
    n
}

#[derive(Clone, Copy, PartialEq)]
enum Rule {
    EvenOdd,
    NonZero,
}
/// Scanline fill; calls span(y, x_first, x_last) (inclusive) for pixels whose CENTRE (x+0.5, y+0.5) is inside.
fn fill_polygon(poly: &[Pt], rule: Rule, mut span: impl FnMut(i32, i32, i32)) {
    let ymin = poly.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor() as i32;
    let ymax = poly.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil() as i32;
    for y in ymin..=ymax {
        let yc = y as f64 + 0.5;
        let mut xs: Vec<(f64, i32)> = vec![];
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let dir = if a.1 <= yc && b.1 > yc {
                1
            } else if b.1 <= yc && a.1 > yc {
                -1
            } else {
                continue;
            };
            xs.push((a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0), dir));
        }
        xs.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
        let (mut wn, mut start) = (0i32, 0.0);
        let inside = |w: i32| {
            if rule == Rule::EvenOdd {
                w & 1 != 0
            } else {
                w != 0
            }
        };
        for (x, d) in xs {
            let was = inside(wn);
            wn += d;
            let now = inside(wn);
            if !was && now {
                start = x;
            }
            if was && !now {
                let (x0, x1) = ((start - 0.5).ceil() as i32, (x - 0.5).ceil() as i32 - 1);
                if x1 >= x0 {
                    span(y, x0, x1);
                }
            }
        }
    }
}
/// Independent oracles: classic ray casting (even-odd) and Sunday's winding number (non-zero).
fn oracle(poly: &[Pt], p: Pt, rule: Rule) -> bool {
    let (mut odd, mut wn) = (false, 0);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            odd = !odd;
        }
        let left = (b.0 - a.0) * (p.1 - a.1) - (p.0 - a.0) * (b.1 - a.1);
        if a.1 <= p.1 && b.1 > p.1 && left > 0.0 {
            wn += 1;
        }
        if a.1 > p.1 && b.1 <= p.1 && left < 0.0 {
            wn -= 1;
        }
    }
    if rule == Rule::EvenOdd { odd } else { wn != 0 }
}

// ---------- curves ----------
fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}
/// de Casteljau for a cubic: returns the point AND the two sub-curves split at t.
fn decasteljau(c: [Pt; 4], t: f64) -> (Pt, [Pt; 4], [Pt; 4]) {
    let (a, b, d) = (
        lerp(c[0], c[1], t),
        lerp(c[1], c[2], t),
        lerp(c[2], c[3], t),
    );
    let (e, f) = (lerp(a, b, t), lerp(b, d, t));
    let p = lerp(e, f, t);
    (p, [c[0], a, e, p], [p, f, d, c[3]])
}
fn quad_to_cubic(q: [Pt; 3]) -> [Pt; 4] {
    [
        q[0],
        lerp(q[0], q[1], 2.0 / 3.0),
        lerp(q[2], q[1], 2.0 / 3.0),
        q[2],
    ]
}
fn dist_line(p: Pt, a: Pt, b: Pt) -> f64 {
    let l = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    if l < 1e-12 {
        return ((p.0 - a.0).powi(2) + (p.1 - a.1).powi(2)).sqrt();
    }
    ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)).abs() / l
}
/// Adaptive flattening: split until both inner control points are within `tol` of the chord.
fn flatten(c: [Pt; 4], tol: f64, out: &mut Vec<Pt>, depth: u32) {
    if depth >= 16 || (dist_line(c[1], c[0], c[3]) <= tol && dist_line(c[2], c[0], c[3]) <= tol) {
        out.push(c[3]);
    } else {
        let (_, l, r) = decasteljau(c, 0.5);
        flatten(l, tol, out, depth + 1);
        flatten(r, tol, out, depth + 1);
    }
}
fn flatten_curve(c: [Pt; 4], tol: f64) -> Vec<Pt> {
    let mut v = vec![c[0]];
    flatten(c, tol, &mut v, 0);
    v
}
/// Uniform Catmull-Rom through pts (endpoints duplicated), as one cubic Bezier per span.
fn catmull_rom(pts: &[Pt]) -> Vec<[Pt; 4]> {
    let mut v = vec![pts[0]];
    v.extend_from_slice(pts);
    v.push(*pts.last().unwrap());
    (1..v.len() - 2)
        .map(|i| {
            let (p0, p1, p2, p3) = (v[i - 1], v[i], v[i + 1], v[i + 2]);
            [
                p1,
                (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0),
                (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0),
                p2,
            ]
        })
        .collect()
}
fn cubic_deriv(c: [Pt; 4], t: f64) -> Pt {
    let u = 1.0 - t;
    let (d0, d1, d2) = (
        (c[1].0 - c[0].0, c[1].1 - c[0].1),
        (c[2].0 - c[1].0, c[2].1 - c[1].1),
        (c[3].0 - c[2].0, c[3].1 - c[2].1),
    );
    (
        3.0 * (u * u * d0.0 + 2.0 * u * t * d1.0 + t * t * d2.0),
        3.0 * (u * u * d0.1 + 2.0 * u * t * d1.1 + t * t * d2.1),
    )
}

// ---------- signed distance functions ----------
fn sd_circle(p: Pt, c: Pt, r: f64) -> f64 {
    ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() - r
}
fn sd_box(p: Pt, c: Pt, half: Pt) -> f64 {
    let (dx, dy) = ((p.0 - c.0).abs() - half.0, (p.1 - c.1).abs() - half.1);
    (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0)
}
fn sd_segment(p: Pt, a: Pt, b: Pt, r: f64) -> f64 {
    dist_seg(p, a, b) - r
}
/// Polynomial smooth minimum: <= min(a,b), and within k/4 of it.
fn smin(a: f64, b: f64, k: f64) -> f64 {
    let h = (k - (a - b).abs()).max(0.0) / k;
    a.min(b) - h * h * k * 0.25
}

// ---------- 5x7 bitmap font ----------
fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        '!' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        _ => [0; 7],
    }
}
/// Draws text at (x, y) with an integer pixel scale; returns the width in pixels.
fn draw_text(c: &mut Canvas, x: i32, y: i32, text: &str, scale: i32, col: Rgb) -> i32 {
    let mut cx = x;
    for ch in text.chars() {
        for (row, bits) in glyph(ch).iter().enumerate() {
            for colm in 0..5 {
                if bits >> (4 - colm) & 1 == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            c.put(cx + colm * scale + sx, y + row as i32 * scale + sy, col);
                        }
                    }
                }
            }
        }
        cx += 6 * scale;
    }
    cx - x - scale
}
fn text_width(text: &str, scale: i32) -> i32 {
    text.chars().count() as i32 * 6 * scale - scale
}

// ---------- tests ----------
fn test_bresenham() {
    let mut rng = Rng(0xDEADBEEF12345);
    for _ in 0..3000 {
        let (x0, y0, x1, y1) = (
            rng.range(0.0, 64.0) as i32,
            rng.range(0.0, 64.0) as i32,
            rng.range(0.0, 64.0) as i32,
            rng.range(0.0, 64.0) as i32,
        );
        let pts = bresenham(x0, y0, x1, y1);
        assert_eq!(pts[0], (x0, y0));
        assert_eq!(*pts.last().unwrap(), (x1, y1));
        assert!(
            pts.windows(2).all(|w| (w[0].0 - w[1].0).abs() <= 1
                && (w[0].1 - w[1].1).abs() <= 1
                && w[0] != w[1]),
            "8-connected"
        );
        assert_eq!(pts.len() as i32, (x1 - x0).abs().max((y1 - y0).abs()) + 1);
        // brute force: for every pixel on the major axis pick the minor-axis pixel closest to the true line
        let (dx, dy) = ((x1 - x0) as f64, (y1 - y0) as f64);
        for &(x, y) in &pts {
            let near = if dx.abs() >= dy.abs() {
                let ty = y0 as f64
                    + if dx != 0.0 {
                        dy * (x - x0) as f64 / dx
                    } else {
                        0.0
                    };
                (0..64)
                    .min_by(|a, b| {
                        ((*a as f64 - ty).abs())
                            .partial_cmp(&(*b as f64 - ty).abs())
                            .unwrap()
                    })
                    .unwrap()
                    - y
            } else {
                let tx = x0 as f64 + dx * (y - y0) as f64 / dy;
                (0..64)
                    .min_by(|a, b| {
                        ((*a as f64 - tx).abs())
                            .partial_cmp(&(*b as f64 - tx).abs())
                            .unwrap()
                    })
                    .unwrap()
                    - x
            };
            assert!(
                near.abs() <= 1,
                "bresenham pixel is more than 1px from the brute-force closest pixel"
            );
        }
    }
    println!(
        "bresenham: 3000 random lines in all octants: 8-connected, endpoints hit, within 1px of brute force"
    );
}

fn test_wu() {
    let mut rng = Rng(77);
    for _ in 0..200 {
        let (a, b) = (
            (rng.range(2.0, 60.0), rng.range(2.0, 60.0)),
            (rng.range(2.0, 60.0), rng.range(2.0, 60.0)),
        );
        let steep = (b.1 - a.1).abs() > (b.0 - a.0).abs();
        let mut sums = std::collections::BTreeMap::new();
        wu_line(a.0, a.1, b.0, b.1, |x, y, c| {
            assert!((0.0..=1.0).contains(&c));
            *sums.entry(if steep { y } else { x }).or_insert(0.0) += c;
        });
        assert!(
            sums.values().all(|s: &f64| (s - 1.0).abs() < 1e-9),
            "Wu coverage per step must sum to 1"
        );
    }
    println!("wu: coverage of the two pixels per major step sums to 1 on 200 random lines");
}

fn test_circles() {
    for r in 1..60 {
        for (x, y) in circle_points(100, 100, r) {
            let d = (((x - 100).pow(2) + (y - 100).pow(2)) as f64).sqrt();
            assert!(
                (d - r as f64).abs() < 0.75,
                "circle r={r} pixel off by {}",
                d - r as f64
            );
        }
    }
    for (a, b) in [(40, 20), (20, 40), (55, 5), (7, 30), (30, 30)] {
        let pts = ellipse_points(0, 0, a, b);
        for &(x, y) in &pts {
            let g = ((x as f64 / a as f64).powi(2) + (y as f64 / b as f64).powi(2)).sqrt();
            assert!(
                (g - 1.0).abs() * (a.min(b) as f64) < 1.2,
                "ellipse {a}x{b} pixel ({x},{y}) off"
            );
        }
        assert!(
            pts.contains(&(a, 0))
                && pts.contains(&(0, b))
                && pts.contains(&(-a, 0))
                && pts.contains(&(0, -b)),
            "axis extremes"
        );
    }
    println!("midpoint circle (r=1..59) and ellipses stay within a pixel of the true curve");
}

fn test_fills() {
    // big canvas, explicit stack: no recursion
    let mut big = Canvas::new(400, 300, [0.0; 3]);
    assert_eq!(flood_fill(&mut big, 0, 0, [1.0, 1.0, 1.0], false), 120_000);
    // 4- vs 8-connectivity across a diagonal wall
    let mut c4 = Canvas::new(64, 64, [0.0; 3]);
    for (x, y) in bresenham(0, 0, 63, 63) {
        c4.put(x, y, [1.0, 0.0, 0.0]);
    }
    let mut c8 = Canvas {
        w: 64,
        h: 64,
        px: c4.px.clone(),
    };
    let n4 = flood_fill(&mut c4, 40, 2, [0.0, 1.0, 0.0], false);
    let n8 = flood_fill(&mut c8, 40, 2, [0.0, 1.0, 0.0], true);
    assert_eq!(
        n4,
        64 * 63 / 2,
        "4-connected fill stays on one side of the diagonal"
    );
    assert_eq!(
        n8,
        64 * 64 - 64,
        "8-connected fill leaks through the diagonal wall"
    );
    // midpoint circle outline: 4-fill stays inside, 8-fill leaks
    let mut cc = Canvas::new(100, 100, [0.0; 3]);
    for (x, y) in circle_points(50, 50, 30) {
        cc.put(x, y, [1.0; 3]);
    }
    let mut cc8 = Canvas {
        w: 100,
        h: 100,
        px: cc.px.clone(),
    };
    let inside = flood_fill(&mut cc, 50, 50, [0.0, 0.0, 1.0], false);
    let leaked = flood_fill(&mut cc8, 50, 50, [0.0, 0.0, 1.0], true);
    assert!(
        inside < (std::f64::consts::PI * 31.0 * 31.0) as usize && leaked > 5000,
        "4-fill {inside}, 8-fill {leaked}"
    );
    println!(
        "flood fill: 400x300 fill ok; diagonal wall 4-conn {n4} px vs 8-conn {n8} px; circle 4-conn {inside} vs 8-conn {leaked}"
    );

    // scanline polygon fill vs oracles on 10k random points
    let mut rng = Rng(4242);
    let (mut checked, mut inside_cnt) = (0, 0);
    for k in 0..100 {
        let n = 4 + (k % 6);
        let poly: Vec<Pt> = (0..n)
            .map(|_| (rng.range(4.0, 60.0), rng.range(4.0, 60.0)))
            .collect();
        for rule in [Rule::EvenOdd, Rule::NonZero] {
            let mut mask = vec![false; 64 * 64];
            fill_polygon(&poly, rule, |y, x0, x1| {
                for x in x0..=x1 {
                    assert!(!mask[(y * 64 + x) as usize], "pixel filled twice");
                    mask[(y * 64 + x) as usize] = true;
                }
            });
            for _ in 0..50 {
                let (px, py) = (rng.range(0.0, 64.0) as i32, rng.range(0.0, 64.0) as i32);
                let want = oracle(&poly, (px as f64 + 0.5, py as f64 + 0.5), rule);
                assert_eq!(
                    mask[(py * 64 + px) as usize],
                    want,
                    "scanline fill disagrees with oracle at ({px},{py})"
                );
                checked += 1;
                inside_cnt += want as usize;
            }
        }
    }
    // the pentagram: centre is inside under non-zero, a hole under even-odd
    let star: Vec<Pt> = (0..5)
        .map(|i| {
            let a = (i * 2) as f64 * std::f64::consts::TAU / 5.0 - 1.57;
            (50.0 + 40.0 * a.cos(), 50.0 + 40.0 * a.sin())
        })
        .collect();
    assert!(
        oracle(&star, (50.0, 50.0), Rule::NonZero) && !oracle(&star, (50.0, 50.0), Rule::EvenOdd)
    );
    println!(
        "polygon fill: {checked} random points agree with ray-casting / winding oracles ({inside_cnt} inside)"
    );
}

fn test_curves() {
    let c = [(2.0, 2.0), (10.0, 90.0), (90.0, -30.0), (98.0, 60.0)];
    let mut prev = 0;
    for tol in [4.0, 1.0, 0.25, 0.05] {
        let poly = flatten_curve(c, tol);
        for i in 0..=2000 {
            let (p, _, _) = decasteljau(c, i as f64 / 2000.0);
            let d = poly
                .windows(2)
                .map(|w| dist_seg(p, w[0], w[1]))
                .fold(f64::MAX, f64::min);
            assert!(d <= tol, "curve point {d} from polyline exceeds tol {tol}");
        }
        assert!(
            poly.len() >= prev,
            "smaller tolerance gives at least as many segments"
        );
        prev = poly.len();
        println!(
            "  flatten tol={tol:<5} -> {:>3} segments (max deviation <= tol)",
            poly.len() - 1
        );
    }
    let (p, l, r) = decasteljau(c, 0.3);
    assert_eq!(l[3], p);
    assert_eq!(r[0], p);
    let (m, _, _) = decasteljau(l, 1.0);
    assert_eq!(m, p, "left half ends at the split point");
    let q = quad_to_cubic([(0.0, 0.0), (5.0, 10.0), (10.0, 0.0)]);
    let (mid, _, _) = decasteljau(q, 0.5);
    assert!(
        (mid.0 - 5.0).abs() < 1e-12 && (mid.1 - 5.0).abs() < 1e-12,
        "quadratic midpoint is (p0+2p1+p2)/4"
    );
    let pts = [
        (0.0, 0.0),
        (10.0, 20.0),
        (25.0, 5.0),
        (40.0, 30.0),
        (60.0, 10.0),
    ];
    let segs = catmull_rom(&pts);
    for (i, s) in segs.iter().enumerate() {
        assert_eq!(s[0], pts[i]);
        assert_eq!(s[3], pts[i + 1], "spline passes through its control points");
        if i + 1 < segs.len() {
            let (d0, d1) = (cubic_deriv(*s, 1.0), cubic_deriv(segs[i + 1], 0.0));
            assert!(
                (d0.0 - d1.0).abs() < 1e-9 && (d0.1 - d1.1).abs() < 1e-9,
                "C1 continuity at joins"
            );
        }
    }
    println!("beziers: flatness tolerance holds; Catmull-Rom interpolates and is C1");
}

fn test_sdf() {
    let mut rng = Rng(9);
    assert!(
        (sd_circle((5.0, 0.0), (0.0, 0.0), 2.0) - 3.0).abs() < 1e-12
            && sd_circle((0.0, 0.0), (0.0, 0.0), 2.0) == -2.0
    );
    assert!(
        sd_box((0.0, 0.0), (0.0, 0.0), (2.0, 1.0)) == -1.0
            && (sd_box((5.0, 5.0), (0.0, 0.0), (2.0, 1.0)) - 5.0).abs() < 1e-12
    );
    assert!((sd_segment((0.0, 3.0), (-5.0, 0.0), (5.0, 0.0), 1.0) - 2.0).abs() < 1e-12);
    for _ in 0..10_000 {
        let (a, b, k) = (
            rng.range(-10.0, 10.0),
            rng.range(-10.0, 10.0),
            rng.range(0.1, 5.0),
        );
        let s = smin(a, b, k);
        assert!(
            s <= a.min(b) + 1e-12
                && s >= a.min(b) - k / 4.0 - 1e-12
                && (s - smin(b, a, k)).abs() < 1e-12
        );
        if (a - b).abs() >= k {
            assert!((s - a.min(b)).abs() < 1e-12, "far apart: plain min");
        }
    }
    println!("sdf: circle/box/segment values and smooth-min bounds verified on 10k random pairs");
}

fn test_text() {
    let mut c = Canvas::new(80, 20, [0.0; 3]);
    let w = draw_text(&mut c, 1, 1, "AB 12", 1, [1.0; 3]);
    assert_eq!(w, text_width("AB 12", 1));
    let lit = c.px.iter().filter(|p| p[0] > 0.5).count() as u32;
    let want: u32 = "AB12"
        .chars()
        .map(|ch| glyph(ch).iter().map(|b| b.count_ones()).sum::<u32>())
        .sum();
    assert_eq!(lit, want, "every glyph bit is drawn exactly once");
    assert!(('A'..='Z').chain('0'..='9').all(|ch| glyph(ch) != [0; 7]) && glyph('a') == glyph('A'));
    println!("font: 5x7 glyphs render ({lit} lit pixels for \"AB12\")");
}

// ---------- composite ----------
fn panel(c: &mut Canvas, ox: i32, oy: i32, title: &str) {
    for x in 0..133 {
        c.put(ox + x, oy, [0.25, 0.27, 0.33]);
        c.put(ox + x, oy + 149, [0.25, 0.27, 0.33]);
    }
    for y in 0..150 {
        c.put(ox, oy + y, [0.25, 0.27, 0.33]);
        c.put(ox + 132, oy + y, [0.25, 0.27, 0.33]);
    }
    draw_text(c, ox + 4, oy + 4, title, 1, [0.9, 0.9, 0.6]);
}

fn composite() -> Canvas {
    let mut c = Canvas::new(400, 300, [0.07, 0.08, 0.11]);
    let hue = |i: usize, n: usize| -> Rgb {
        let h = i as f32 / n as f32 * 6.0;
        let f = |k: f32| (((h + k) % 6.0 - 3.0).abs() - 1.0).clamp(0.0, 1.0);
        [f(0.0), f(4.0), f(2.0)]
    };
    // 1: lines
    let (ox, oy) = (0, 0);
    panel(&mut c, ox, oy, "LINES");
    let ctr = (ox as f64 + 66.0, oy as f64 + 85.0);
    for i in 0..24 {
        let a = i as f64 * std::f64::consts::TAU / 24.0;
        let (ex, ey) = (ctr.0 + 55.0 * a.cos(), ctr.1 + 55.0 * a.sin());
        if i % 2 == 0 {
            for (x, y) in bresenham(ctr.0 as i32, ctr.1 as i32, ex as i32, ey as i32) {
                c.put(x, y, hue(i, 24));
            }
        } else {
            wu_line(ctr.0, ctr.1, ex, ey, |x, y, cov| {
                c.blend(x, y, hue(i, 24), cov)
            });
        }
    }
    // 2: circles + thick
    let (ox, oy) = (133, 0);
    panel(&mut c, ox, oy, "CIRCLES");
    for (x, y) in circle_points(ox + 45, oy + 55, 30) {
        c.put(x, y, [0.4, 0.9, 0.6]);
    }
    for (x, y) in ellipse_points(ox + 66, oy + 55, 55, 20) {
        c.put(x, y, [0.9, 0.7, 0.3]);
    }
    let zig = [
        (10.0, 130.0),
        (35.0, 100.0),
        (60.0, 135.0),
        (85.0, 100.0),
        (120.0, 130.0),
    ];
    for (i, w) in zig.windows(2).enumerate() {
        thick_line(
            &mut c,
            (ox as f64 + w[0].0, oy as f64 + w[0].1 - 10.0),
            (ox as f64 + w[1].0, oy as f64 + w[1].1 - 10.0),
            4.0 - i as f64 * 0.7,
            hue(i * 3 + 1, 12),
        );
    }
    // 3: polygons
    let (ox, oy) = (266, 0);
    panel(&mut c, ox, oy, "EVEN-ODD / NONZERO");
    for (k, rule) in [Rule::EvenOdd, Rule::NonZero].into_iter().enumerate() {
        let (cx, cy) = (ox as f64 + 34.0 + 65.0 * k as f64, oy as f64 + 85.0);
        let star: Vec<Pt> = (0..5)
            .map(|i| {
                let a = (i * 2) as f64 * std::f64::consts::TAU / 5.0 - 1.57;
                (cx + 30.0 * a.cos(), cy + 30.0 * a.sin())
            })
            .collect();
        fill_polygon(&star, rule, |y, x0, x1| {
            for x in x0..=x1 {
                c.put(
                    x,
                    y,
                    if k == 0 {
                        [0.9, 0.4, 0.5]
                    } else {
                        [0.4, 0.6, 0.95]
                    },
                );
            }
        });
    }
    // 4: curves
    let (ox, oy) = (0, 150);
    panel(&mut c, ox, oy, "BEZIER / CATMULL-ROM");
    let off = |p: Pt| (p.0 + ox as f64, p.1 + oy as f64);
    let cub = [
        off((8.0, 70.0)),
        off((30.0, 15.0)),
        off((95.0, 120.0)),
        off((125.0, 40.0)),
    ];
    for w in cub.windows(2) {
        for (x, y) in bresenham(w[0].0 as i32, w[0].1 as i32, w[1].0 as i32, w[1].1 as i32) {
            c.put(x, y, [0.3, 0.3, 0.4]);
        }
    }
    let poly = flatten_curve(cub, 0.3);
    for w in poly.windows(2) {
        wu_line(w[0].0, w[0].1, w[1].0, w[1].1, |x, y, cov| {
            c.blend(x, y, [0.95, 0.85, 0.3], cov)
        });
    }
    let pts: Vec<Pt> = [
        (10.0, 140.0),
        (35.0, 100.0),
        (60.0, 135.0),
        (90.0, 95.0),
        (124.0, 130.0),
    ]
    .iter()
    .map(|&p| off(p))
    .collect();
    for s in catmull_rom(&pts) {
        let f = flatten_curve(s, 0.3);
        for w in f.windows(2) {
            wu_line(w[0].0, w[0].1, w[1].0, w[1].1, |x, y, cov| {
                c.blend(x, y, [0.4, 0.9, 0.9], cov)
            });
        }
    }
    for p in &pts {
        for (x, y) in circle_points(p.0 as i32, p.1 as i32, 2) {
            c.put(x, y, [1.0, 0.4, 0.4]);
        }
    }
    // 5: SDF blob
    let (ox, oy) = (133, 150);
    panel(&mut c, ox, oy, "SDF SMOOTH UNION");
    for y in 14..148 {
        for x in 2..131 {
            let p = (x as f64, y as f64);
            let d = smin(
                smin(
                    sd_circle(p, (45.0, 70.0), 22.0),
                    sd_box(p, (82.0, 85.0), (20.0, 14.0)),
                    24.0,
                ),
                sd_segment(p, (30.0, 120.0), (100.0, 40.0), 4.0),
                12.0,
            );
            let col: Rgb = if d < 0.0 {
                [0.9, 0.55 + 0.002 * (-d as f32).min(40.0), 0.25]
            } else {
                let b = 0.5 + 0.5 * (d * 0.6).cos();
                [
                    0.15 + 0.2 * b as f32,
                    0.2 + 0.25 * b as f32,
                    0.4 + 0.3 * b as f32,
                ]
            };
            let edge = (1.0 - d.abs()).clamp(0.0, 1.0);
            c.put(ox + x, oy + y, col);
            c.blend(ox + x, oy + y, [1.0; 3], edge);
        }
    }
    // 6: flood fill + text
    let (ox, oy) = (266, 150);
    panel(&mut c, ox, oy, "FLOOD FILL + FONT");
    for (x, y) in circle_points(ox + 38, oy + 55, 26) {
        c.put(x, y, [1.0; 3]);
    }
    for w in [(70, 30), (120, 30), (120, 80), (70, 80), (95, 55), (70, 30)].windows(2) {
        for (x, y) in bresenham(ox + w[0].0, oy + w[0].1, ox + w[1].0, oy + w[1].1) {
            c.put(x, y, [1.0; 3]);
        }
    }
    flood_fill(&mut c, ox + 38, oy + 55, [0.3, 0.7, 0.4], false);
    flood_fill(&mut c, ox + 98, oy + 40, [0.8, 0.4, 0.3], false);
    draw_text(&mut c, ox + 8, oy + 100, "FLOWFORGE", 2, [0.95, 0.95, 1.0]);
    draw_text(
        &mut c,
        ox + 8,
        oy + 122,
        "5X7 FONT 0123",
        1,
        [0.6, 0.8, 1.0],
    );
    c
}

fn main() {
    println!("== 2D drawing primitives ==");
    test_bresenham();
    test_wu();
    test_circles();
    test_fills();
    test_curves();
    test_sdf();
    test_text();
    let c = composite();
    let rgb = c.to_rgb8();
    assert!(
        c.px.iter().filter(|p| **p != [0.07, 0.08, 0.11]).count() > 5000,
        "composite has content"
    );
    save_image("r254_primitives", c.w, c.h, &rgb);
    ascii_preview(c.w, c.h, &rgb, 80);
    println!("all drawing-primitive assertions passed");
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
