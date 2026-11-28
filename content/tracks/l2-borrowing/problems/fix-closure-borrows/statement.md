`Meter` doesn't compile. Each of its three methods uses a closure, and each closure borrows something
for longer, or more broadly, than the code around it expects. Fix them. `walk` must stay generic over
the closure type (no `dyn`), no reading may be copied into a temporary collection, and no `Cell` or
`RefCell`.

One fix that looks right for `walk` compiles, and then fails to build for a different reason. Read that
error carefully.
