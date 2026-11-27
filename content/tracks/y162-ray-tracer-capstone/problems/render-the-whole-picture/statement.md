**Capstone, step 9 of 9.** The starter contains reference code for steps 1 to 8, plus `Stream`. This last step puts it all together: a picture, computed on several threads.

## `render`

The picture has `width * height` pixels in linear light, row after row from the **top**, each row from the left.

For a pixel in column `x` and row `y`, take `samples` samples:

1. Get places inside the pixel with `pixel_offsets`.
2. For each place `(dx, dy)`, the ray goes through `s = (x + dx) / width`, `t = (y + dy) / height`. Take its lens point from `unit_disk`.
3. Follow it with `trace` or `path`, as `settings.integrator` says.

The pixel is the **average** of its samples.

**All random numbers** of a pixel come from `Stream::for_pixel(settings.seed, x, y)`.

Return `None` if `width`, `height` or `samples` is 0, if there are more than `MAX_PIXELS` pixels, or more than `MAX_SAMPLES` samples. No size may panic or overflow. Refuse before allocating.

## Threads

Use up to `settings.threads` threads (0 means 1). Never start more than `MAX_THREADS`, or more than there are rows. The picture must be **exactly the same, bit for bit**, for every thread count. Rows take very unequal time, so a thread that finishes should take the next row.

The tests compare the bits. They cannot see how you share the work among threads, so that part is on your honour. The part "Colour one sheet with many hands" grades it on its own.

## `rmse`

The root mean squared error: the square root of the average, over all pixels **and all three colours**, of the squared difference. `None` if the pictures differ in length or are empty.

## What the tests check

They compare **your** pictures with each other, with exact values where the scene has them, and within stated bounds elsewhere. They never compare with a stored image. In the Cornell box, 16 samples must have less than half the error of 1 sample. In theory it is a quarter.

When it passes, change the tests' scene to 400 by 400 with 200 samples, run it in a release build, and save the PNG. You built that.
