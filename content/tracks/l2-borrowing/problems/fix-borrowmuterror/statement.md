`Scheduler` compiles, and three of its methods panic with `already borrowed`. Each holds a `RefCell`
guard longer than it looks: one in a `let`, one in the condition of an `if let`, one in the condition of
a `while let`. Fix them without `try_borrow*`, without cloning, and without changing what the methods do.
