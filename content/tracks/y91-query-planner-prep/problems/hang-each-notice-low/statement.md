A school building is a tree. The entrance leads to two parts, each part to two smaller ones, and so on down to single rooms. A notice is meant for some rooms. It must hang at the **lowest** place that contains all of them, so that as few people as possible walk past it.

`hang(building, notices)` returns one answer for each notice, in order. An answer is the list of turns from the entrance to that place. For a notice that names a room the building does not have, the answer is `None`.

- Room numbers in a building are below 64, and no number appears twice.
- A notice may name any `u8`, and may name a room twice.
- A notice for no room at all stays at the entrance (`Some` of no turns).

The building has at most 64 rooms. There can be 20 000 notices.
