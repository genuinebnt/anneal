This is capstone step 3. The first plan for a query is lazy: pair every row of every table, then filter. The first improvement is to check each condition **as soon as possible**.

A `Logical` plan is a tree.

`Scan(t)` reads table `t`. `Filter` keeps rows. `Cross` pairs every left row with every right row. `Join` keeps the pairs for which its condition holds.

**`tables_of(condition)`** returns the tables a condition reads, one bit each. Table numbers are below 64. Each table is scanned at most once in a plan.

**`push_down(plan)`** returns a plan with the same meaning:

- The tree of tables is unchanged: the same pairs, with the same left and right sides.
- Take every part (see `conjuncts`) of every `Filter` and `Join`, at any depth. Each appears exactly once in the result.
- Each part sits at the **lowest** node that has all the tables it reads. A part that reads no table may sit on any one table.
- A `Filter` appears only directly on a `Scan`, and holds all that table's parts. A scan without parts has no `Filter`.
- A pair of inputs is a `Join` if it has parts, and a `Cross` if it has none.
- A node with one part holds that part itself. With two or more it holds one `All` of them, in any order.

An `Any` is a single part: never take it apart. An empty `All` has no parts, so it disappears.

In the plans you are given, a condition reads only tables of the plan. There can be 500 filters stacked on each other.

Running `push_down` on its own result gives the same plan. Only the order of the parts inside a node may differ.

**If you also take the BusTub course:** its optimizer challenge *Where does the predicate go?* is close to this step. Whichever you do first will help with the other.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
