# Graphics

3D math, image formats, rasterization, ray tracing, 2D drawing, fractals.

Run any recipe from the `cookbook/` directory: `cargo run --release --example <name>` (the name is the file name without `.rs`). Each file starts with a header: what it shows, `LEVEL`, `COMMON USES` and `Read:` links.

6 examples.

| Example | What it shows | Level | Common uses | Crates |
|---|---|---|---|---|
| [`vec3_matrix_quaternion`](vec3_matrix_quaternion.rs) | 3D math from scratch: Vec3, Mat4, projections, quaternions, gimbal lock, ray intersections | intermediate | games (cameras, skeletal animation with slerp), CAD/CAM viewers, film VFX and offline renderers, robotics | std only |
| [`png_ppm_writer`](png_ppm_writer.rs) | write and read images without crates: PPM (P6) and a minimal PNG encoder + decoder, sRGB, ASCII preview | intermediate | ray/path tracers and simulators dumping frames, ML data generation (synthetic images), game tools, | std only |
| [`software_rasterizer`](software_rasterizer.rs) | a complete software rasterizer: transforms, near-plane clipping, edge functions, z-buffer, shading, textures | advanced | games and engines (the same stages run on GPUs), CAD/CAE viewers, film previs, GIS terrain viewers, | std only |
| [`lines_curves_fills`](lines_curves_fills.rs) | 2D drawing primitives: lines, circles, fills, Bezier and Catmull-Rom curves, SDFs, bitmap text | intermediate | vector graphics and font rasterisers, plotting libraries, GIS map rendering, CAD sketchers, game UI and | std only |
| [`image_processing`](image_processing.rs) | classic image processing: convolution, edges, histograms, dithering, resampling, colour spaces, alpha | intermediate | photo editors and camera pipelines, video scalers and codecs (YCbCr), GPU texture filtering and mipmaps, | std only |
| [`fractals_noise_automata`](fractals_noise_automata.rs) | generative graphics: fractals, noise, L-systems, cellular automata, reaction-diffusion, contours | intermediate | games (terrain, caves, clouds, textures from noise), film VFX (procedural materials), GIS (contour lines, | std only |
