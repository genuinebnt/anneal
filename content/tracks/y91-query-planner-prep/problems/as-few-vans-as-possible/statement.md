A courier must deliver some parcels today, and every van trip carries one load. What a van may carry is decided by rules you do not see: weight, size, goods that must stay apart, and some stranger ones. You only get `fits(load)`, which answers for one exact load.

Parcels are numbered `0..parcels`, with `parcels` at most 12. A load is a `u32` with one bit for each parcel in it.

`fewest_vans(parcels, fits)` returns a list of loads that carries **every parcel exactly once**, with every load fitting, and as few loads as possible. Any such list is fine. It returns `None` if no list works. With no parcels the answer is an empty list.

Assume **nothing** about `fits`. A load may fit although a part of it does not, and two loads that fit may not fit together. Asking twice gives the same answer.

The tests count the calls. You may call `fits` at most `3^parcels + 1` times, and never with the empty load or with unknown parcels. Going over fails at once.
