**Capstone, step 8 of 9.** The starter contains reference code for steps 1 to 7.

So far, light travels from a lamp to a surface and stops. In a real room it keeps bouncing: a white ceiling lit from below lights everything else. This step follows it.

## Random directions

`cosine_direction(normal, rng)` returns a random unit vector on the side of the unit `normal`. Directions near the normal are likelier. The density per solid angle is `cos(angle to normal) / pi`.

## One path

`path(scene, ray, max_bounces, rng)` returns one **random estimate** of the light arriving back along the ray. The average of many calls must approach the true value. `ray.dir` may have any non-zero length. Look for hits with `t > 0.0001` along the unit direction.

The path visits at most `max_bounces` surfaces. When it must stop there, it returns what it has gathered. If it hits nothing, it gathers the sky and ends. Lamps in `scene.lamps` are **not** used: here, light comes only from the sky and from `Light` surfaces.

At each surface:

- `Light`: gather its radiance (from either side) and end.
- `Matte`, `Checker`, `Shiny`: a perfectly matte surface with its `surface_colour`. It sends on that share of each colour. Continue in a `cosine_direction`. The highlight of `Shiny` is ignored.
- `Mirror`: continue in the bounced direction, filtered by `tint`.
- `Glass`: continue **one** way, bounced or crossing. Choose so that on average each gets the share from `reflectance`. If nothing crosses, bounce. If the hit is from the back, filter by `e^(-absorb * distance)`.

## Exact answers

Follow the cosine density, and some scenes give the true value in **every single** call. A matte ball under a sky of 1 shows exactly its colour. The tests demand that within `1e-12`.

## Knowing when to stop

`max_bounces` can be large. Most paths must end long before it. In a closed room with no light, with `max_bounces = 10,000`, the average path may ask `rng` for at most **200** numbers. This holds even if the walls give back **all** the light.

Ending paths early must **not** change the average.

It must not add much noise either. A path's **weight** is the share of any light found next that would reach the camera. It starts at 1.

In a grey scene, take a surface where the weight, filtered by that surface, is still 0.9 or more. There the path goes on with a chance of **at least 0.9**. The tests of averages rely on this.

Tests of averages leave room for the noise of any estimator that follows these rules.
