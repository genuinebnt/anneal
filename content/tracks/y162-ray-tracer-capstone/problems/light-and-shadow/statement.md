**Capstone, step 5 of 9.** The starter contains reference code for steps 1 to 4. It now defines materials, lamps and a scene. This step works out what a matte surface looks like.

## Colours

`surface_colour` is the matte colour of a material at a point.

- `Matte` and `Shiny`: their `albedo`.
- `Checker`: squares on the ground, `1 / cells_per_unit` wide. Take `floor(x * cells_per_unit) + floor(z * cells_per_unit)`. If it is even, the colour is `even`, else `odd`.
- No point may panic in `surface_colour`, however large. Beyond `1e15`, or for NaN, either colour is fine.
- `Mirror`, `Glass`, `Light`: none.

## Shading

`shade_matte` returns zero if the material has no matte colour. Otherwise it is `colour * AMBIENT`, plus a part for each lamp.

For one point `q` of a lamp, let `l` be the unit vector from `point` to `q`, `d` the distance, and `f = normal . l`. The part is zero if `f <= 0`, if `q` is not in view, or if `d` is 0. Otherwise it is:

```text
power  x  ( colour * f  +  glint * (1, 1, 1) )  /  d^2
```

Here `x` multiplies colours component by component.

`glint` is 0, except for `Shiny`. There, let `h` be the unit vector halfway between `l` and `toward_eye`. Then `glint = strength * max(0, normal . h)^sharpness`.

## Shadows

`q` is in view if nothing in the world lies on the straight way from `point` to `q`. **Every** surface blocks, glass and glowing ones too.

- Things within `0.0001` of either end are free. Things farther than `0.001` from both ends **must** block. In between is not tested.
- `point` usually comes from a ray hit, so it lies a hair above or below its surface. That surface must **never** shadow it from a lamp clearly in front. This includes a floor, and a ball of radius 1000.
- Nothing beyond the lamp blocks.

## Lamps

A `Point` lamp has one point. For a `Panel`, take `max(panel_samples, 1)` points, each **evenly at random** over the panel, and **average** their parts. `rng` returns numbers in `[0, 1)`. Its call order and count are free.

Exact results are compared within `1e-9`. Tests of panels allow five standard deviations.

This is the classic "Whitted" shading: lamps are not visible objects, and light that bounces off other surfaces is only imitated by the ambient term. A later step does better.
