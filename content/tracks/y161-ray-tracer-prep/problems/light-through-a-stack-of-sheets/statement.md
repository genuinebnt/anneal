A window maker stacks panes of different clear materials. A thin beam of light hits the stack at an angle. How much of it goes **straight through**, and where does it come out?

All faces are flat and parallel. The same material, with index `outside`, is on both sides. `angle` is measured in radians from the direction at right angles to the faces. The sheets are listed in the order the beam meets them.

## At each face

There are faces before the first sheet, between neighbours, and after the last.

- **Bending** follows Snell's law: `index * sin(angle)` is the same on both sides. If that would need a sine above 1, everything is reflected: return `None`.
- **Reflection:** the share `R` bounces off and is lost to us. `1 - R` goes on. Use Schlick's estimate:

```text
R = r0 + (1 - r0) (1 - c)^5      r0 = ((n1 - n2) / (n1 + n2))^2
```

`n1` and `n2` are the two indices. `c` is the cosine of the beam's angle **on the side of the face with the smaller index**. Where both sides have the same index there is no face, so `R = 0`.

## Inside each sheet

The beam travels in a straight line from one face to the other. Over a length `d` it keeps the share `e^(-absorb * d)`. It also moves sideways, **along the faces**.

Return the total sideways `shift` and the total `share` that leaves the far side. Light that bounces more than once is ignored. With no sheets, the shift is 0 and the share is 1.

Also return `None` for bad input: an angle outside `[0, pi/2)`, an index that is not a positive number, a negative thickness or `absorb`, or any NaN or infinity.

Tests compare within `1e-9` unless a case is close to total reflection. Exactly critical angles are not tested.
