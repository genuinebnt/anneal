A map app shows a 3D city. It has to draw a name tag over each building, so it must know where on the screen each point of the world appears.

A `Viewer` stands at `eye` and looks towards `target`. `up` says which way is up in the picture. It need not be at right angles to the view or have length 1.

The picture is `width` by `height` pixels, with square pixels. `vfov_degrees` is the angle from its **bottom edge to its top edge**.

This is ordinary perspective: straight lines from the eye, onto a flat screen that faces the view direction.

`on_screen(viewer, point)` returns:

- `across`: pixels from the left edge, from 0 to `width`;
- `down`: pixels from the **top** edge, from 0 to `height`;
- `depth`: how far ahead of the eye the point is, **measured along the view direction**. It is not the distance to the eye.

Return `None` when the point does not show: `depth` is `1e-9` or less, or it falls outside the picture. Edge positions within `1e-6` pixels are not tested.

Also return `None` when the viewer cannot see:

- a side of the picture is 0;
- `vfov_degrees` is not strictly between 0 and 180;
- `eye` equals `target`, or `up` is zero;
- the sine of the angle between `up` and the view is `1e-9` or less;
- a number is NaN.

Tests allow an error of `1e-6` pixels.
