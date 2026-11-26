# Practice: graphics

How to use this page: run the recipe, open the generated gallery images (and the ASCII previews), then re-write one routine from the header description alone.
Graphics bugs look plausible, so every task asks for a check that is not "it looks right": exact pixel equality with a brute-force reference, a round trip, a conservation property or a checksum.
Order: [S] about 30 minutes, [M] about 2 hours, [A] a weekend evening or more. No solutions are given.

Image oracle recipe: render with the fast routine and with an obviously correct (slow, per-pixel, floating-point) routine, then assert max absolute difference per channel under a bound you can justify, and save the diff image when it fails.

---

## fractals_noise_automata

**Prove you got it**
1. Why does the cardioid/bulb shortcut give identical results to full iteration, and what is the worst-case saving (which fraction of the image is skipped on the default view)?
2. Why is the quintic fade `6t^5 - 15t^4 + 10t^3` used instead of smoothstep `3t^2 - 2t^3` in Perlin noise (what is continuous at lattice points)?

**Try next**
- [S] Add 3-octave fractal Brownian motion variants and assert the output stays within a bound you derive (sum of amplitudes).
- [M] Verify `marching_squares` contours are closed loops on a smooth field and that every segment endpoint lies on a cell edge; oracle: count sign changes per cell.
- [A] Generate a terrain heightmap (fbm + domain warping), colour by height, add hill-shading from a light direction, write PNG, and produce 3 seeds. Compare noise quality with [noise-rs](https://github.com/Razaekel/noise-rs).

**Use in**: procedural textures and terrain, generative art, GIS contours.

**Pitfall**: hashing lattice coordinates with a weak hash, giving visible grid artefacts; check by plotting the autocorrelation.

---

## image_processing

**Prove you got it**
1. Why are separable Gaussian blur and the full 2D kernel identical (assert), and what is the multiply count per pixel for k = 15?
2. Why must blurring and resizing happen in linear light and not on raw sRGB bytes? Compute the mid-grey you get when averaging black and white in each space.

**Try next**
- [S] Add a median filter 3x3 and show it removes salt-and-pepper noise while a Gaussian does not; measure PSNR before/after.
- [M] Add border-handling modes (clamp, wrap, mirror) as an enum and a test that a constant image is a fixed point of every kernel with sum 1 under all modes.
- [A] Implement a full pipeline: sRGB decode, resize with Lanczos3, unsharp mask, Floyd-Steinberg to 1-bit, PNG output; compare resize quality to [image](https://github.com/image-rs/image) and [imageproc](https://github.com/image-rs/imageproc) via SSIM you implement.

**Use in**: thumbnails, e-ink and print, preprocessing for vision.

**Pitfall**: premultiplied versus straight alpha when blending: premultiply before filtering.

---

## lines_curves_fills

**Prove you got it**
1. For Bresenham, what is the error term update and why are both endpoints always hit? Compare against an exact rational-arithmetic oracle (as `oracle` does for fills).
2. Why do Wu coverage values sum to 1 in each column, and where does that break (near-vertical lines)?

**Try next**
- [S] Draw an anti-aliased circle by supersampling 4x4 and compare to an SDF-based coverage (`sd_circle`) at the same resolution.
- [M] Flatten a cubic Bezier with a tolerance; assert the polyline stays within the tolerance of the true curve for 10^4 random curves (sample the curve densely as oracle).
- [A] Build a mini SVG-path renderer (M, L, C, Z) with even-odd and non-zero fills and stroke width; compare to output from [tiny-skia](https://github.com/RazrFalcon/tiny-skia) on 10 shapes and read how [lyon](https://github.com/nical/lyon) tessellates.

**Use in**: plotting libraries, vector UI, font rendering.

**Pitfall**: filling polygons with pixel centres at integer coordinates versus +0.5 (off-by-half shifts every edge).

---

## png_ppm_writer

**Prove you got it**
1. Compute by hand the CRC-32 of the 4-byte string "IEND" and compare with the known last chunk of every PNG (AE 42 60 82).
2. What does the Paeth filter predict, and why do filters improve compression though they do not compress themselves?

**Try next**
- [S] Add the Sub, Up and Average filters one at a time and assert the decode round trip for random images including 1xN and Nx1.
- [M] Implement dynamic Huffman blocks in `deflate` and compare output size against stored and fixed on a screenshot-like image.
- [A] Make your decoder robust: feed it 10^4 mutated PNGs (byte flips of valid files) and require "error, never panic, never hang"; compare behaviour with the [png](https://github.com/image-rs/image-png) crate on the same corpus. Related: `encoding_compression/compression_lz77_huffman`.

**Use in**: dumping frames from simulators and renderers.

**Pitfall**: the PNG stream is big-endian for lengths and CRC, while most of your other formats are little-endian.

---

## software_rasterizer

**Prove you got it**
1. Why clip against the near plane in clip space before the divide by w?
2. How does the top-left fill rule guarantee that a pixel on a shared edge is drawn exactly once? Describe the `test_fill_rule` check.

**Try next**
- [S] Add back-face culling and assert the cube draws 3 visible faces at most from any camera.
- [M] Add perspective-correct texture interpolation and verify against a ray-cast per-pixel oracle on a textured quad (error bound under 1/255 per channel except at edges).
- [A] Add a depth-buffer-based shadow map, render a scene of 10^5 triangles, and profile where time goes (edge setup versus per-pixel work); try tile-based rasterisation with threads (`concurrency/rayon_data_parallelism`) and report speed-up. Study Dmitry Sokolov's [tinyrenderer](https://github.com/ssloy/tinyrenderer) for a comparable course.

**Use in**: previews, deterministic CPU fallback, learning what a GPU does.

**Pitfall**: interpolating colours or UVs linearly in screen space (not divided by w) which swims on large triangles.

---

## vec3_matrix_quaternion

**Prove you got it**
1. Show `q` and `-q` represent the same rotation, then explain what slerp must do about it.
2. At pitch = 90 degrees which two Euler angles collapse into one, and how does `euler_from_matrix` signal it?

**Try next**
- [S] Test `Mat4::inverse` against `M * M^-1 = I` on 10^4 random well-conditioned matrices and report the worst error.
- [M] Add quaternion-to-matrix and back; assert round trips for 10^4 random unit quaternions up to sign.
- [A] Write a tiny ray tracer for spheres and triangles with shadow rays, reflections and Schlick Fresnel; avoid shadow acne with a t_min epsilon and demonstrate the artefact without it. Compare API design with [glam](https://github.com/bitshifter/glam-rs) and [nalgebra](https://github.com/dimforge/nalgebra), and look at the bigger version in `capstones/ray_tracer`.

**Use in**: cameras, skeletal animation, robotics.

**Pitfall**: row-major versus column-major and row versus column vectors; write one asymmetric matrix test to pin the convention.

---

## Chapter projects

### Small (about 2 hours): "image toolbox CLI"
Combine `png_ppm_writer`, `image_processing` and `dev_tools/clap_cli`.
- Milestones: (1) read PPM/PNG; (2) commands `blur`, `edges`, `resize`, `dither`; (3) write PNG.
- Acceptance: each command's output equals the recipe's slow reference within 1 level per channel on a gradient image; a corrupted file exits with code 1 and a message, no panic.
- Stretch: histogram equalisation; side-by-side contact sheet.

### Intermediate (about a weekend): "2D plot and vector renderer"
Combine `lines_curves_fills`, `png_ppm_writer`, `fractals_noise_automata` (contours), and `math_science/numerics_and_math` (functions to plot).
- Milestones: (1) axes and tick marks with bitmap text; (2) line charts with anti-aliasing; (3) contour plot of a 2D function with marching squares; (4) legend and PNG export.
- Acceptance: the rendered plot of `sin` has maximum vertical error under 1 pixel against a high-resolution sampled curve; contour lines are closed or end at the border.
- Stretch: SVG path input; compare to [tiny-skia](https://github.com/RazrFalcon/tiny-skia) renderings.

### Advanced (about a week): "software 3D engine"
Combine `vec3_matrix_quaternion`, `software_rasterizer`, `png_ppm_writer`, `math_science/computational_geometry` (ear clipping and hulls for meshes), `math_science/spatial_search_structures` (BVH/kd for ray tests), and `concurrency/rayon_data_parallelism`.
- Milestones: (1) OBJ loader (`text_and_parsing`); (2) camera with quaternion orbit; (3) z-buffer rasteriser with texture mapping; (4) BVH ray-traced reference render of the same scene; (5) threaded tiles.
- Acceptance: rasterised and ray-traced silhouettes agree on at least 99% of pixels; frame render time scales with thread count up to the memory bandwidth limit.
- Stretch: shadow maps, normal maps; read [tinyrenderer](https://github.com/ssloy/tinyrenderer) and compare.
