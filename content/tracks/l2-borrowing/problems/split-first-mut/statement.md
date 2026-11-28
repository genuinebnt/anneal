None of these three functions compiles: each one needs a mutable borrow of one part of a slice while
another part is borrowed too. Fix them without copying any `String`, building a new buffer, or moving values
around with `mem::take`, `mem::replace` or `swap`.
`seal_frames` also has one bug the compiler can't see; the doc comments are the spec.
