**Capstone, step 6 of 9.** The starter contains reference code for steps 1 to 5. This step follows light through mirrors and glass, and finishes the first of the two renderers.

## Three tools

`reflect(dir, normal)` bounces `dir` off a surface with unit `normal`. The part along the surface stays. The part along the normal turns around.

`refract(dir, normal, eta)` is Snell's law. `dir` and `normal` are unit vectors, and `normal` points against `dir`.

`eta = from / to`, the two indices of refraction. The outgoing unit vector crosses to the other side. Its part along the surface is `eta` times the incoming one. If that part would be longer than 1, nothing crosses: return `None`.

`reflectance(cos_in, from, to)` estimates the share of light that bounces, with Schlick's formula:

```text
r0 + (1 - r0) (1 - c)^5      where  r0 = ((from - to) / (from + to))^2
```

`c` is the cosine of the angle to the normal **on the side with the smaller index**. Going into the denser side, `c = cos_in`. Going out of it, `c` is the cosine of the **outgoing** angle. If nothing crosses, the share is 1.

## Following a ray

`trace(scene, ray, depth, panel_samples, rng)` returns the light that arrives back along the ray. `ray.dir` may have any non-zero length. Look for hits with `t > 0.0001` along the unit direction.

- **Nothing hit:** the sky in that direction.
- **The side of the ray:** the hit is from the **front** if the ray runs against the surface's normal. Everything below uses the normal turned towards the ray.
- `Light`: its `radiance`, from either side.
- `Matte`, `Checker`, `Shiny`: `shade_matte`, seen from where the ray came.
- `Mirror`: `tint` times the trace of the bounced ray, with `depth - 1`. With `depth` 0, black.
- `Glass`: with `depth` 0, black. The outside always has index 1. Mix the bounced ray and the crossing ray, both traced with `depth - 1`, by `reflectance`. If nothing crosses, take the bounced ray alone.
- **Loss in glass:** if a glass surface is hit from the **back**, the ray travelled there inside. Multiply the whole result by `e^(-absorb * distance)`, colour by colour.

`depth` above `MAX_DEPTH` (64) counts as 64. No value may overflow the stack in a scene of two mirrors. Tests use at most 40 where glass can branch twice.

**Limits to know.** Glass inside glass is treated as glass in air. Two glass objects can make the work double at every turn.

Tests compare exact cases within `1e-9` and the glass ball within `1e-6`.
