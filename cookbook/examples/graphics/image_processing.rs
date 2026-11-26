//! RECIPE 255 (graphics): classic image processing: convolution, edges, histograms, dithering, resampling, colour spaces, alpha.
//! LEVEL: intermediate
//! Use in: photo/video tools, computer vision pre-processing, thumbnails, e-ink and print, texture pipelines, medical imaging.   Run: cargo run --release --example image_processing
#![allow(dead_code)]
//!
//! CONVOLUTION: each output pixel is a weighted sum of its neighbourhood. A GAUSSIAN kernel is the outer product of
//! two 1D Gaussians, so a k x k blur can be done as a horizontal pass then a vertical pass: 2k multiplies per pixel
//! instead of k*k, identical result (we assert both). SOBEL: two 3x3 kernels estimate the x and y derivative, the
//! gradient magnitude is the edge strength. UNSHARP MASK: out = img + amount * (img - blur(img)), boosting the detail
//! the blur removed (and overshooting at edges, which is why it looks "sharp").
//! HISTOGRAM EQUALISATION: map each level through the normalised cumulative histogram so levels spread evenly.
//! DITHERING to 1 bit: Floyd-Steinberg pushes each pixel's quantisation error onto 4 unvisited neighbours (7/16, 3/16,
//! 5/16, 1/16), so local average brightness is preserved; ordered (Bayer) dithering compares to a tiled threshold matrix.
//! RESAMPLING uses a separable reconstruction kernel: nearest (box), bilinear (triangle), bicubic Catmull-Rom (cubic
//! that passes through samples), Lanczos3 (windowed sinc: sharpest, but its negative lobes cause RINGING: values
//! overshoot past the original range next to hard edges). Downscaling widens the kernel to avoid aliasing.
//! COLOUR: sRGB values are gamma-encoded (roughly value^(1/2.2)); light adds up in LINEAR space, so blurring, resizing
//! and blending must be done there: averaging black and white in sRGB gives 0.5 encoded = only 21% of the light.
//! RGB<->HSV and RGB<->YCbCr (BT.601) are invertible re-parameterisations. ALPHA: store colour PREMULTIPLIED by alpha;
//! then "over" is just s + d*(1-a_s), associative, and interpolation does not leak colour from transparent pixels.
//! COMMON USES: photo editors and camera pipelines, video scalers and codecs (YCbCr), GPU texture filtering and mipmaps,
//! game UI compositing, e-ink/printer halftoning, edge detection ahead of computer-vision steps, medical-imaging windowing.
//! Read: https://www.redblobgames.com/ and https://github.com/ssloy/tinyrenderer/wiki

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

#[derive(Clone)]
struct Img {
    w: usize,
    h: usize,
    d: Vec<f32>,
}
impl Img {
    fn new(w: usize, h: usize, v: f32) -> Img {
        Img {
            w,
            h,
            d: vec![v; w * h],
        }
    }
    fn at(&self, x: i64, y: i64) -> f32 {
        // clamp-to-edge
        self.d[y.clamp(0, self.h as i64 - 1) as usize * self.w
            + x.clamp(0, self.w as i64 - 1) as usize]
    }
    fn mean(&self) -> f32 {
        self.d.iter().sum::<f32>() / self.d.len() as f32
    }
    fn max_abs_diff(&self, o: &Img) -> f32 {
        self.d
            .iter()
            .zip(&o.d)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max)
    }
    fn map(&self, f: impl Fn(f32) -> f32) -> Img {
        Img {
            w: self.w,
            h: self.h,
            d: self.d.iter().map(|&v| f(v)).collect(),
        }
    }
    fn transpose(&self) -> Img {
        let mut o = Img::new(self.h, self.w, 0.0);
        for y in 0..self.h {
            for x in 0..self.w {
                o.d[x * self.h + y] = self.d[y * self.w + x];
            }
        }
        o
    }
    fn crop(&self, x0: usize, y0: usize, w: usize, h: usize) -> Img {
        let mut o = Img::new(w, h, 0.0);
        for y in 0..h {
            for x in 0..w {
                o.d[y * w + x] = self.at((x0 + x) as i64, (y0 + y) as i64);
            }
        }
        o
    }
}

/// A deterministic test picture: gradient, disc, bars, checker, diagonal stripes.
fn test_image(w: usize, h: usize) -> Img {
    let mut im = Img::new(w, h, 0.0);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let mut v = 0.15 + 0.5 * fx * (0.6 + 0.4 * fy);
            let (dx, dy) = (fx - 0.3, fy - 0.35);
            if dx * dx + dy * dy < 0.04 {
                v = 0.92;
            }
            if (0.55..0.9).contains(&fx) && (0.1..0.4).contains(&fy) {
                v = if ((x / 5) + (y / 5)) % 2 == 0 {
                    0.95
                } else {
                    0.1
                };
            }
            if fy > 0.65 && fy < 0.9 && fx > 0.1 && fx < 0.9 {
                v = if ((x + y) / 3) % 2 == 0 { 0.85 } else { 0.25 } * (0.5 + 0.5 * fx);
            }
            im.d[y * w + x] = v;
        }
    }
    im
}

// ---------- convolution ----------
fn gaussian_1d(sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil() as i64;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
        .collect();
    let s: f32 = k.iter().sum();
    k.iter().map(|v| v / s).collect()
}
/// Direct 2D convolution (clamp-to-edge). Returns the image and the number of multiplications.
fn conv2d(img: &Img, k: &[f32], kw: usize, kh: usize) -> (Img, u64) {
    let mut o = Img::new(img.w, img.h, 0.0);
    let (rx, ry) = ((kw / 2) as i64, (kh / 2) as i64);
    let mut mults = 0;
    for y in 0..img.h as i64 {
        for x in 0..img.w as i64 {
            let mut s = 0.0;
            for j in 0..kh as i64 {
                for i in 0..kw as i64 {
                    s += k[j as usize * kw + i as usize] * img.at(x + i - rx, y + j - ry);
                    mults += 1;
                }
            }
            o.d[y as usize * img.w + x as usize] = s;
        }
    }
    (o, mults)
}
/// Separable: 1D pass along x, then along y.
fn conv_separable(img: &Img, k: &[f32]) -> (Img, u64) {
    let pass = |im: &Img| {
        let mut o = Img::new(im.w, im.h, 0.0);
        let r = (k.len() / 2) as i64;
        for y in 0..im.h as i64 {
            for x in 0..im.w as i64 {
                o.d[y as usize * im.w + x as usize] = (0..k.len() as i64)
                    .map(|i| k[i as usize] * im.at(x + i - r, y))
                    .sum();
            }
        }
        o
    };
    let o = pass(&img.transpose()).transpose(); // vertical pass via transpose
    let o = pass(&o);
    (o, 2 * (img.w * img.h * k.len()) as u64)
}
fn outer(k: &[f32]) -> Vec<f32> {
    k.iter()
        .flat_map(|a| k.iter().map(move |b| a * b))
        .collect()
}
fn box_kernel(n: usize) -> Vec<f32> {
    vec![1.0 / (n * n) as f32; n * n]
}
fn sobel(img: &Img) -> Img {
    let gx = [-1.0, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];
    let gy = [-1.0, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];
    let (a, _) = conv2d(img, &gx, 3, 3);
    let (b, _) = conv2d(img, &gy, 3, 3);
    Img {
        w: img.w,
        h: img.h,
        d: a.d
            .iter()
            .zip(&b.d)
            .map(|(x, y)| (x * x + y * y).sqrt())
            .collect(),
    }
}
fn unsharp(img: &Img, sigma: f32, amount: f32) -> Img {
    let (b, _) = conv_separable(img, &gaussian_1d(sigma));
    Img {
        w: img.w,
        h: img.h,
        d: img
            .d
            .iter()
            .zip(&b.d)
            .map(|(o, b)| o + amount * (o - b))
            .collect(),
    }
}

// ---------- histogram ----------
fn histogram(img: &Img) -> [u32; 256] {
    let mut h = [0u32; 256];
    for &v in &img.d {
        h[(v.clamp(0.0, 1.0) * 255.0).round() as usize] += 1;
    }
    h
}
fn equalize(img: &Img) -> Img {
    let h = histogram(img);
    let (mut cdf, mut acc) = ([0u32; 256], 0);
    for i in 0..256 {
        acc += h[i];
        cdf[i] = acc;
    }
    let cmin = *cdf.iter().find(|&&c| c > 0).unwrap();
    let n = img.d.len() as u32;
    img.map(|v| {
        let i = (v.clamp(0.0, 1.0) * 255.0).round() as usize;
        (cdf[i] - cmin) as f32 / (n - cmin).max(1) as f32
    })
}
fn std_dev(img: &Img) -> f32 {
    let m = img.mean();
    (img.d.iter().map(|v| (v - m) * (v - m)).sum::<f32>() / img.d.len() as f32).sqrt()
}

// ---------- dithering ----------
fn floyd_steinberg(img: &Img) -> Img {
    let mut w = img.clone();
    let mut out = Img::new(img.w, img.h, 0.0);
    for y in 0..img.h {
        for x in 0..img.w {
            let old = w.d[y * img.w + x];
            let new = if old >= 0.5 { 1.0 } else { 0.0 };
            out.d[y * img.w + x] = new;
            let err = old - new;
            for (dx, dy, f) in [(1i64, 0i64, 7.0), (-1, 1, 3.0), (0, 1, 5.0), (1, 1, 1.0)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx >= 0 && ny >= 0 && (nx as usize) < img.w && (ny as usize) < img.h {
                    w.d[ny as usize * img.w + nx as usize] += err * f / 16.0;
                }
            }
        }
    }
    out
}
/// Bayer matrix of side 2^n, values 0..side^2.
fn bayer(n: u32) -> Vec<Vec<u32>> {
    let mut m = vec![vec![0u32]];
    for _ in 0..n {
        let s = m.len();
        let mut b = vec![vec![0; s * 2]; s * 2];
        for y in 0..s {
            for x in 0..s {
                let v = 4 * m[y][x];
                b[y][x] = v;
                b[y][x + s] = v + 2;
                b[y + s][x] = v + 3;
                b[y + s][x + s] = v + 1;
            }
        }
        m = b;
    }
    m
}
fn ordered_dither(img: &Img, m: &[Vec<u32>]) -> Img {
    let (s, area) = (m.len(), (m.len() * m.len()) as f32);
    let mut o = Img::new(img.w, img.h, 0.0);
    for y in 0..img.h {
        for x in 0..img.w {
            let t = (m[y % s][x % s] as f32 + 0.5) / area;
            o.d[y * img.w + x] = if img.d[y * img.w + x] > t { 1.0 } else { 0.0 };
        }
    }
    o
}

// ---------- resampling ----------
fn k_nearest(x: f32) -> f32 {
    if (-0.5..0.5).contains(&x) { 1.0 } else { 0.0 }
}
fn k_bilinear(x: f32) -> f32 {
    (1.0 - x.abs()).max(0.0)
}
fn k_catmull(x: f32) -> f32 {
    let x = x.abs();
    if x < 1.0 {
        1.5 * x * x * x - 2.5 * x * x + 1.0
    } else if x < 2.0 {
        -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
    } else {
        0.0
    }
}
fn k_lanczos3(x: f32) -> f32 {
    let sinc = |t: f32| {
        if t.abs() < 1e-6 {
            1.0
        } else {
            (std::f32::consts::PI * t).sin() / (std::f32::consts::PI * t)
        }
    };
    if x.abs() < 3.0 {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}
struct Kernel {
    name: &'static str,
    f: fn(f32) -> f32,
    support: f32,
}
const KERNELS: [Kernel; 4] = [
    Kernel {
        name: "nearest",
        f: k_nearest,
        support: 0.5,
    },
    Kernel {
        name: "bilinear",
        f: k_bilinear,
        support: 1.0,
    },
    Kernel {
        name: "catmull-rom",
        f: k_catmull,
        support: 2.0,
    },
    Kernel {
        name: "lanczos3",
        f: k_lanczos3,
        support: 3.0,
    },
];
fn resample_x(src: &Img, nw: usize, k: &Kernel) -> Img {
    let scale = src.w as f32 / nw as f32;
    let fs = scale.max(1.0); // widen the kernel when shrinking
    let mut o = Img::new(nw, src.h, 0.0);
    for ox in 0..nw {
        let c = (ox as f32 + 0.5) * scale - 0.5;
        let (lo, hi) = (
            (c - k.support * fs).ceil() as i64,
            (c + k.support * fs).floor() as i64,
        );
        let ws: Vec<(i64, f32)> = (lo..=hi).map(|i| (i, (k.f)((i as f32 - c) / fs))).collect();
        let sum: f32 = ws.iter().map(|w| w.1).sum();
        for y in 0..src.h {
            o.d[y * nw + ox] = ws
                .iter()
                .map(|&(i, w)| w * src.at(i, y as i64))
                .sum::<f32>()
                / sum;
        }
    }
    o
}
fn resample(src: &Img, nw: usize, nh: usize, k: &Kernel) -> Img {
    resample_x(&resample_x(src, nw, k).transpose(), nh, k).transpose()
}

// ---------- colour ----------
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
type Rgb = [f32; 3];
fn rgb_to_hsv(c: Rgb) -> [f32; 3] {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == c[0] {
        60.0 * (((c[1] - c[2]) / d).rem_euclid(6.0))
    } else if mx == c[1] {
        60.0 * ((c[2] - c[0]) / d + 2.0)
    } else {
        60.0 * ((c[0] - c[1]) / d + 4.0)
    };
    [h, if mx == 0.0 { 0.0 } else { d / mx }, mx]
}
fn hsv_to_rgb(h: [f32; 3]) -> Rgb {
    let (c, hp) = (h[2] * h[1], h[0] / 60.0);
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = h[2] - c;
    [r + m, g + m, b + m]
}
fn rgb_to_ycbcr(c: Rgb) -> Rgb {
    let y = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    [y, 0.5 + (c[2] - y) / 1.772, 0.5 + (c[0] - y) / 1.402]
}
fn ycbcr_to_rgb(c: Rgb) -> Rgb {
    let (y, cb, cr) = (c[0], c[1] - 0.5, c[2] - 0.5);
    let (r, b) = (y + 1.402 * cr, y + 1.772 * cb);
    [r, (y - 0.299 * r - 0.114 * b) / 0.587, b]
}

// ---------- alpha ----------
#[derive(Clone, Copy, Debug)]
struct Rgba {
    c: Rgb,
    a: f32,
} // straight (non-premultiplied)
#[derive(Clone, Copy, Debug)]
struct Pre {
    c: Rgb,
    a: f32,
} // colour already multiplied by alpha
fn premul(p: Rgba) -> Pre {
    Pre {
        c: p.c.map(|v| v * p.a),
        a: p.a,
    }
}
fn unpremul(p: Pre) -> Rgba {
    Rgba {
        c: if p.a > 0.0 {
            p.c.map(|v| v / p.a)
        } else {
            [0.0; 3]
        },
        a: p.a,
    }
}
fn over_pre(s: Pre, d: Pre) -> Pre {
    Pre {
        c: [0, 1, 2].map(|i| s.c[i] + d.c[i] * (1.0 - s.a)),
        a: s.a + d.a * (1.0 - s.a),
    }
}
fn over_straight(s: Rgba, d: Rgba) -> Rgba {
    let a = s.a + d.a * (1.0 - s.a);
    Rgba {
        c: if a > 0.0 {
            [0, 1, 2].map(|i| (s.c[i] * s.a + d.c[i] * d.a * (1.0 - s.a)) / a)
        } else {
            [0.0; 3]
        },
        a,
    }
}

// ---------- tests ----------
fn rand_rgb(r: &mut Rng) -> Rgb {
    [r.f(), r.f(), r.f()]
}

fn test_convolution(img: &Img) {
    let k1 = gaussian_1d(1.5);
    assert!((k1.iter().sum::<f32>() - 1.0).abs() < 1e-5 && k1.len() == 2 * 5 + 1);
    let (direct, m2) = conv2d(img, &outer(&k1), k1.len(), k1.len());
    let (sep, ms) = conv_separable(img, &k1);
    let diff = direct.max_abs_diff(&sep);
    assert!(diff < 1e-5, "separable must equal direct 2D, diff {diff}");
    assert!(ms * 4 < m2, "separable does far fewer multiplications");
    println!(
        "gaussian blur (k={}): direct {} mults, separable {} mults ({:.1}x fewer), max diff {:.1e}",
        k1.len(),
        m2,
        ms,
        m2 as f64 / ms as f64,
        diff
    );
    let (b, _) = conv2d(img, &box_kernel(5), 5, 5);
    assert!(
        (b.mean() - img.mean()).abs() < 0.01,
        "blur preserves mean brightness"
    );
    assert!(std_dev(&b) < std_dev(img), "blur reduces contrast");
    // flat image -> sobel 0 ; vertical step -> response only near the step
    let flat = Img::new(16, 16, 0.4);
    assert!(sobel(&flat).d.iter().all(|&v| v.abs() < 1e-6));
    let mut step = Img::new(16, 16, 0.0);
    for y in 0..16 {
        for x in 8..16 {
            step.d[y * 16 + x] = 1.0;
        }
    }
    let s = sobel(&step);
    assert!((0..16).all(|y| s.d[y * 16 + 7] > 3.0
        && s.d[y * 16 + 8] > 3.0
        && s.d[y * 16 + 3] < 1e-6
        && s.d[y * 16 + 12] < 1e-6));
    // unsharp: flat unchanged, step overshoots past [0,1]
    assert!(unsharp(&flat, 1.5, 1.0).max_abs_diff(&flat) < 1e-5);
    let u = unsharp(&step, 1.5, 1.0);
    assert!(
        u.d.iter().cloned().fold(0.0, f32::max) > 1.05
            && u.d.iter().cloned().fold(1.0, f32::min) < -0.05,
        "unsharp overshoots at edges"
    );
    println!(
        "sobel: flat -> 0, step edge localised; unsharp mask overshoots {:.2} at a step",
        u.d.iter().cloned().fold(0.0, f32::max)
    );
}

fn test_histogram(img: &Img) -> Img {
    let low = img.map(|v| 0.25 + 0.35 * v); // compress into a narrow band
    let eq = equalize(&low);
    let (hl, he) = (histogram(&low), histogram(&eq));
    let span = |h: &[u32; 256]| {
        (
            h.iter().position(|&c| c > 0).unwrap(),
            h.iter().rposition(|&c| c > 0).unwrap(),
        )
    };
    let (sl, _) = span(&hl);
    let (a, b) = span(&he);
    assert!(
        b - a > 250 && (b - a) > (span(&hl).1 - sl) * 2,
        "equalisation spreads levels to the full range"
    );
    assert!(std_dev(&eq) > std_dev(&low) * 1.5, "contrast increases");
    println!(
        "histogram: low-contrast spans levels {}..{}; equalised spans {}..{} (std {:.3} -> {:.3})",
        sl,
        span(&hl).1,
        a,
        b,
        std_dev(&low),
        std_dev(&eq)
    );
    // tiny ASCII histogram (16 bins)
    for (name, h) in [("before", &hl), ("after ", &he)] {
        let bins: Vec<u32> = (0..16)
            .map(|i| h[i * 16..(i + 1) * 16].iter().sum())
            .collect();
        let mx = *bins.iter().max().unwrap() as f32;
        let bars: String = bins
            .iter()
            .map(|&b| b" .:-=+*#%@"[((b as f32 / mx) * 9.0).round() as usize] as char)
            .collect();
        println!("  {name} |{bars}|");
    }
    eq
}

fn test_dither() -> (Img, Img) {
    let img = test_image(100, 100);
    let fs = floyd_steinberg(&img);
    let m = bayer(3);
    let mut flat: Vec<u32> = m.iter().flatten().cloned().collect();
    flat.sort();
    assert_eq!(
        flat,
        (0..64).collect::<Vec<u32>>(),
        "Bayer 8x8 is a permutation of 0..63"
    );
    let ord = ordered_dither(&img, &m);
    assert!(
        fs.d.iter().chain(&ord.d).all(|&v| v == 0.0 || v == 1.0),
        "1-bit output"
    );
    let naive = img.map(|v| if v >= 0.5 { 1.0 } else { 0.0 });
    let (m0, mfs, mo, mn) = (img.mean(), fs.mean(), ord.mean(), naive.mean());
    assert!(
        (m0 - mfs).abs() < 0.01 && (m0 - mo).abs() < 0.03,
        "dithers preserve mean brightness"
    );
    println!(
        "dither mean brightness: source {m0:.3}, floyd-steinberg {mfs:.3}, bayer {mo:.3}, plain threshold {mn:.3}"
    );
    // on a smooth ramp plain thresholding is far worse
    let mut ramp = Img::new(128, 16, 0.0);
    for y in 0..16 {
        for x in 0..128 {
            ramp.d[y * 128 + x] = x as f32 / 127.0;
        }
    }
    let e = |i: &Img| (i.mean() - ramp.mean()).abs();
    assert!(e(&floyd_steinberg(&ramp)) < 0.005 && e(&ordered_dither(&ramp, &m)) < 0.02);
    (fs, ord)
}

fn test_resample() -> Img {
    let img = test_image(64, 64);
    for k in &KERNELS[1..] {
        let same = resample(&img, 64, 64, k);
        let tol = if k.name == "bilinear" || k.name == "catmull-rom" {
            1e-5
        } else {
            1e-4
        };
        assert!(
            same.max_abs_diff(&img) < tol,
            "{} at scale 1 reproduces the image",
            k.name
        );
    }
    // partition of unity for bilinear and Catmull-Rom
    for t in 0..=10 {
        let f = t as f32 / 10.0;
        for k in [k_bilinear as fn(f32) -> f32, k_catmull] {
            let s: f32 = (-3..=3).map(|i| k(f - i as f32)).sum();
            assert!((s - 1.0).abs() < 1e-5);
        }
    }
    // ringing on a hard step
    let mut step = Img::new(16, 1, 0.0);
    for x in 8..16 {
        step.d[x] = 1.0;
    }
    println!("resampling a hard step 16 -> 64 wide (range of output; original is 0..1):");
    for k in &KERNELS {
        let r = resample(&step, 64, 1, k);
        let (mn, mx) = (
            r.d.iter().cloned().fold(f32::MAX, f32::min),
            r.d.iter().cloned().fold(f32::MIN, f32::max),
        );
        println!("  {:<12} min {:>7.4}  max {:>7.4}", k.name, mn, mx);
        match k.name {
            "nearest" | "bilinear" => {
                assert!(mn >= -1e-6 && mx <= 1.0 + 1e-6, "{} never rings", k.name)
            }
            _ => assert!(
                mn < -0.01 && mx > 1.01,
                "{} rings (overshoot beyond 0..1)",
                k.name
            ),
        }
    }
    // downscale: constant stays constant, mean roughly preserved
    let big = test_image(128, 128);
    let small = resample(&big, 32, 32, &KERNELS[3]);
    assert!(
        (small.mean() - big.mean()).abs() < 0.02,
        "downscale keeps mean"
    );
    // the 8th panel: a 25x25 crop upscaled 4x with Lanczos3
    resample(&img.crop(8, 8, 25, 25), 100, 100, &KERNELS[3])
}

fn test_colour() {
    let mut r = Rng(0xC0FFEE);
    for _ in 0..10_000 {
        let c = rand_rgb(&mut r);
        let back = hsv_to_rgb(rgb_to_hsv(c));
        assert!(
            (0..3).all(|i| (back[i] - c[i]).abs() < 1e-4),
            "HSV round trip {c:?} -> {back:?}"
        );
        let y = ycbcr_to_rgb(rgb_to_ycbcr(c));
        assert!(
            (0..3).all(|i| (y[i] - c[i]).abs() < 1e-4),
            "YCbCr round trip"
        );
        let l = srgb_to_linear(c[0]);
        assert!((linear_to_srgb(l) - c[0]).abs() < 1e-5, "sRGB round trip");
        let hsv = rgb_to_hsv(c);
        assert!((0.0..360.0).contains(&hsv[0]) && (0.0..=1.0).contains(&hsv[1]));
    }
    let g = rgb_to_ycbcr([0.3, 0.3, 0.3]);
    assert!(
        (g[1] - 0.5).abs() < 1e-6 && (g[2] - 0.5).abs() < 1e-6,
        "grey has no chroma"
    );
    assert_eq!(rgb_to_hsv([0.4, 0.4, 0.4])[1], 0.0);
    // blur black/white stripes: sRGB-space average is DARKER than linear-light average
    let mut stripes = Img::new(32, 8, 0.0);
    for y in 0..8 {
        for x in 0..32 {
            stripes.d[y * 32 + x] = (x % 2) as f32;
        }
    }
    let k = gaussian_1d(2.0);
    let (naive, _) = conv_separable(&stripes, &k); // blur the encoded values directly
    let lin = stripes.map(srgb_to_linear);
    let (lin_blur, _) = conv_separable(&lin, &k);
    let correct = lin_blur.map(linear_to_srgb);
    let (n, c) = (naive.d[4 * 32 + 16], correct.d[4 * 32 + 16]);
    assert!(
        n < c - 0.1,
        "blurring in sRGB ({n:.3}) is darker than in linear light ({c:.3})"
    );
    println!(
        "stripe blur (displayed value): in sRGB {n:.3}  vs  in linear light {c:.3}  (50% light = 0.735 encoded)"
    );
    // alpha
    let mut worst = 0.0f32;
    for _ in 0..10_000 {
        let mk = |r: &mut Rng| Rgba {
            c: rand_rgb(r),
            a: r.f(),
        };
        let (a, b, c) = (premul(mk(&mut r)), premul(mk(&mut r)), premul(mk(&mut r)));
        let (l, rr) = (over_pre(over_pre(a, b), c), over_pre(a, over_pre(b, c)));
        worst = worst.max(
            (0..3)
                .map(|i| (l.c[i] - rr.c[i]).abs())
                .fold((l.a - rr.a).abs(), f32::max),
        );
        let (s, d) = (unpremul(a), unpremul(b));
        let st = over_straight(s, d);
        let pr = unpremul(over_pre(a, b));
        assert!(
            (st.a - pr.a).abs() < 1e-5
                && (0..3).all(|i| (st.c[i] - pr.c[i]).abs() < 2e-3 || st.a < 0.01),
            "straight and premultiplied agree"
        );
    }
    assert!(
        worst < 1e-6,
        "premultiplied over is associative (worst {worst:e})"
    );
    // interpolation of a transparent-black texel with an opaque-white one
    let (t, w) = (
        Rgba {
            c: [0.0; 3],
            a: 0.0,
        },
        Rgba {
            c: [1.0; 3],
            a: 1.0,
        },
    );
    let mid_straight = Rgba {
        c: [0, 1, 2].map(|i| (t.c[i] + w.c[i]) / 2.0),
        a: 0.5,
    };
    let (pt, pw) = (premul(t), premul(w));
    let mid_pre = Pre {
        c: [0, 1, 2].map(|i| (pt.c[i] + pw.c[i]) / 2.0),
        a: 0.5,
    };
    let white = Pre {
        c: [1.0; 3],
        a: 1.0,
    };
    let (bad, good) = (
        over_pre(premul(mid_straight), white).c[0],
        over_pre(mid_pre, white).c[0],
    );
    assert!(
        (good - 1.0).abs() < 1e-6 && bad < 0.8,
        "straight-alpha lerp darkens (fringe): {bad} vs {good}"
    );
    println!(
        "alpha: premultiplied over associative (worst {worst:.1e}); half-transparent white over white: premult {good:.2}, straight-lerp {bad:.2}"
    );
}

fn gray_rgb(img: &Img) -> Vec<[u8; 3]> {
    img.d
        .iter()
        .map(|&v| {
            let b = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            [b, b, b]
        })
        .collect()
}

fn main() {
    println!("== image processing ==");
    let img = test_image(100, 100);
    test_convolution(&img);
    let eq = test_histogram(&img);
    let (fs, ord) = test_dither();
    let up = test_resample();
    test_colour();

    // before/after strip: 4x2 panels of 100x100
    let (blur, _) = conv_separable(&img, &gaussian_1d(2.0));
    let edges = sobel(&img).map(|v| (v * 1.2).min(1.0));
    let sharp = unsharp(&img, 1.5, 1.5);
    let panels = [&img, &blur, &edges, &sharp, &eq, &fs, &ord, &up];
    let (w, h) = (400usize, 200usize);
    let mut rgb = vec![0u8; w * h * 3];
    for (i, p) in panels.iter().enumerate() {
        let (ox, oy) = ((i % 4) * 100, (i / 4) * 100);
        for (j, px) in gray_rgb(p).iter().enumerate() {
            let (x, y) = (ox + j % 100, oy + j / 100);
            rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(px);
        }
    }
    println!(
        "panels: original | gaussian blur | sobel | unsharp || equalised | floyd-steinberg | bayer | lanczos3 4x crop"
    );
    save_image("r255_strip", w, h, &rgb);
    ascii_preview(w, h, &rgb, 80);
    println!("all image-processing assertions passed");
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
