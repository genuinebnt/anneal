This is the last capstone step. Planners are easy to get subtly wrong, and a wrong plan gives wrong rows with no error. Build the tool that finds such mistakes.

`find_wrong_answer(planner, seed, tries)` searches for a database and a query where the plan returned by `planner` gives other rows than the query means. The starter gives you `MemStore`, `catalog_of`, `run` and `plain_answer`.

A `Case` you return must:

- have at most `CASE_TABLES` (4) tables, each with at most `CASE_ROWS` (8) rows and `CASE_WIDTH` (3) columns;
- hold a valid query: `optimize` accepts it with `catalog_of(tables)`;
- really fail: with `planner`'s plan, `run` and `plain_answer` give different rows. Order does not matter, but how often each row appears does.

Ask the planner **at most `tries` times**. Return `None` if you find nothing. The same `seed` and planner must give the same result every time: an identical `Case`, field for field.

The faulty planners in the tests build correct-looking plans with unusual choices. Some always use an index when there is one. Some always use one kind of join. Each has **one** mistake, such as:

- a boundary that is off by one in a range or a comparison;
- a condition that is lost or only partly checked;
- a join that assumes an order its inputs do not have;
- a plan that loses rows with missing values;
- rows that come out the wrong number of times, or columns in the wrong places.

Each must be caught within 3000 tries, for several seeds. Correct planners with the same unusual choices must never be accused. A planner here is any function from a query and its catalog to a plan.

**A limit of these tests.** You can read every faulty planner in them, so a few hand-made cases could pass, whatever the seed. The tests cannot tell that from a real search. Building the search is the point of this step.

**About the starter.** It holds reference code for the earlier capstone steps, so this step can be solved and checked on its own. Reading it gives those steps away. You write only the parts marked `todo!()`.
