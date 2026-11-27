A simulation throws things at random. It needs two kinds of throw, and each has an exact spread.

`roll()` returns random numbers in `[0, 1)`. Use as many as you like for one throw.

## On the table

`on_table(roll)` returns a point of the round table `x^2 + y^2 <= 1`. **Any two regions of the table with the same area are equally likely.**

## Toward the sky

`toward_sky(roll)` returns a direction `[x, y, z]` of length 1 with `z >= 0`. Steeper directions are likelier: the density per area of the dome is **in proportion to `z`**. No direction around the vertical is favoured.

As a check on your reading: the share of directions with height above `h` is `1 - h^2`.

The tests draw 60,000 times with a seeded `roll` and compare shares of draws with areas. They allow five standard deviations, so a correct spread passes. They also mix in rolls at the very ends of the range.
