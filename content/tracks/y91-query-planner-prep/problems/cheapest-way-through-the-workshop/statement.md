A parcel goes through a row of stations, in order. Each station has several machines, and the parcel must use exactly one at each. It starts in wrapping `0`.

A `Machine` has three facts:

- `needs`: the wrapping the parcel must have to go in. `None` takes any wrapping.
- `leaves`: the wrapping the parcel has when it comes out. `None` leaves it as it was inside the machine.
- `price`: what using it costs.

If the parcel does not have the wrapping a machine needs, it is rewrapped first. Rewrapping into anything costs `rewrap`.

`cheapest_route(workshop, rewrap)` returns the lowest total price, with the number of the machine used at each station. If several routes cost that, any one of them is fine. It returns `None` if the parcel cannot get through. With no stations the total is 0.

You see the workshop through a trait, and the tests **count the calls of `machine`**. You may look at the machines at most 300 times as often as there are machines (plus 300). That is plenty for some designs and far too little for trying every route.

There can be 1000 stations. Every total fits in a `u64`.
