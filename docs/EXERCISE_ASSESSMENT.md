# Are the BusTub exercises good? An assessment (2026-10-11)

Question from the owner: *are the exercises really good?* This is measured from the repository and from a small mutation experiment;
it is not a judgement of how well the course teaches, which only working through it shows.

## Verdict

The exercises are well built. Every learn and build stage has at least 5 tests (median 8; 1,389 tests over 169 core stages), every
assertion names the behaviour it checks (0 bare assertions), most stages pair example tests with a model or property test, the tests
state a contract and leave the design open, and the stage-by-stage pass counts of the newest module behave exactly as intended
(0, 7, 13, 21, 32 tests pass as the stages are solved).

Three things need attention: **some tests miss mistakes in the edges of their own stage** (5 untested behaviours found, section 3),
**a bug that loops forever in the extendible hash table would hang a learner's run instead of failing** (section 3), and **two stages
are far larger than the rest** (section 4). None is a reason to rework the course.

## 1. What was measured, and what was wrong the first time

| measure | result |
|---|---|
| core stages / tests | 169 stages (124 build, 31 boss, 14 learn), 1,389 tests, plus 148 optional challenges at 5 tests or more |
| tests per learn/build stage | median 8, minimum 5, maximum 18 |
| boss stages with fewer than 4 tests | 5 (1d-04, 2a-05, 3c-05, 3i-08, 3j-07); each runs a whole script or property, so the count understates them |
| bare assertions | 0 (`tools/add_assert_messages.py --check`) |
| stages with a model or property test | at least 104 of 164 (by name, an undercount) |
| concurrency tests / crash-at-every-step tests | about 74 / 21 (counted by pattern, so approximate) |

**A correction.** A first count said 28 stages had fewer than 5 tests. That was wrong: it did not see tests written inside
`proptest!` blocks or driven by a SQL script. With a looser pattern the thin stages disappear, apart from the five bosses above.
Counting tests by a regular expression is only a rough check; `cargo test -- --list` is the exact one.

## 2. Freedom and what restricts it

- **The test files state contracts** (public items named on the stage page); every core stage page has a "Your freedom" section.
- **Error types are mostly given.** The template ships 18 concrete error types (16 enums, 2 structs) besides the engine's `Exception`, and about 100 assertion lines in
  the stage tests name a specific variant or kind.
  Only module 3k asks the learner to design an error type, and its 32 tests check traits and behaviour, never fields.
- **Tests that pin the inside.** Several modules assert exact tree shapes (`leaf_sizes`, `depth`: 22 assertions in 2c, 6 in 2d),
  exact latch counts (11 in 2c, and BusTub's own insert and delete tests), directory depths (18 in 2b) and pin counts (34 in 1f and 1g,
  20 more in BusTub's buffer pool test). (Numbers are lines of the test files that mention the observer.) These follow BusTub's own tests and are part of what is being taught, but a learner who designs a different valid structure
  will meet them. Naming them on the stage page ("Fixed by the tests") would turn a surprise into a stated rule; not done yet.

## 3. Do the tests catch mistakes? A mutation experiment

Method: copy the reference solution, change one thing at a time in the code of one stage (flip a comparison, drop a `+ 1`, delete a
call), and run the stage's own test binary. A mutant that still passes *survived*. Seven files, 6 mutants each, random but seeded
(`tools/course_mutation.py`, run one build at a time). 40 mutants ran.

| outcome | count |
|---|---|
| caught (a test failed) | 28 |
| detected only as a hang (the run never finished; see below) | 1 |
| survived | 11 |

**The hang.** Inverting a comparison in the bucket-split redistribution of the extendible hash table made the tests loop for the
whole 8 minutes of the experiment. The stage 2b tests have no timeout (the B+ tree tests use a `within(60 s, ..)` wrapper that turns a
hang into a failure with a message). A learner with the same bug would see a terminal that never returns. That is the bug a split
written by hand is most likely to have, so it is worth a wrapper.

Every survivor was read by hand. Three are **equivalent** (no behaviour change, so not the tests' fault): a dead branch in
`LockManager::try_lock`, an argument the function ignores (`get_next_tuple_offset`'s `_meta`), and a read loop that stops on an empty
read. One changes **only the file size** (the disk manager grows its file one allocation later, using its spare page). That leaves
**7 surviving mutants, which point at 5 untested behaviours**:

| behaviour no test pins | mutants that passed | where |
|---|---|---|
| **casting a string to a decimal at all**: a numeric string with a fraction, an exponent, or a value beyond the decimal range | 3 (every decimal parse failing; the fraction ignored; the range check off) | `types/value.rs`, stage 3a-02 |
| a decimal exactly equal to the smallest value of the target type (`-127.0` to TINYINT casts, `-128.0` is the NULL encoding and does not) | 1 | `types/value.rs`, stage 3a-02 |
| a tuple that would fit **exactly** (the page filled to the last byte) | 1 | `storage/page/table_page.rs`, stage 3b-03 |
| marking a tuple deleted through the in-place update path counts it as deleted (the tests use that path only with `is_deleted: false`) | 1 | `storage/page/table_page.rs`, stage 3b-03 |
| releasing a **read** guard must not mark the page dirty | 1 | `storage/page/page_guard.rs`, stage 1g-01 |

Caveats, so these are not over-read: 40 mutants is a sample, and one operator family. The run used only the stage's own test file,
so a behaviour might also be exercised by a later stage or a SQL script; the gap is "not pinned where it is taught". The first
first row is the most telling: all three of its mutants passed, so no test of 3a-02 casts any text to a decimal. The tests there
are written around integers.

## 4. Pacing: two outliers

Median solution size of a learn/build stage is 38 lines (75th percentile 57, 90th 86). Two stages are far beyond it:

| stage | solution lines | files | tests | why |
|---|---|---|---|---|
| 2c-01 "One leaf is a tree" | 555 | 5 | 6 | the learner designs all three page types (header, internal, leaf) and the first tree operations |
| 2b-02 "Three levels, no splits yet" | 355 | 4 | 6 | the learner designs header, directory and bucket pages and an invariant checker |

Both are deliberate: designing the page types is the lesson, and they are labelled hard. The risk is that they are where a learner
can stall for hours with only 6 tests of feedback. Size against label is otherwise sane (median 18 lines easy, 37 medium, 48 hard).
Nine more stages are over 100 lines (3a-01, 2c-04, 3d-04, 0b-01, 3i-01, 3i-03, 3a-02, 4d-05, 4c-01).

## 5. Recommendations, in order

1. **Add the missing tests** for the 5 behaviours to the owning stages (3a-02, 3b-03, 1g-01), and a timeout wrapper to the 2b tests,
   as 2c has. Small and additive; each new test fails on its mutant and passes on the reference. Needs the template regenerated and
   those stages re-verified (`course verify --stage`).
2. **Rerun the mutation check on the files not sampled** (the B+ tree and the buffer pool were only lightly sampled), then
   triage the same way. It is cheap once the scratch copy exists.
3. **Name what the tests fix** in a short line beside "Your freedom" on the stages that pin shapes, depths, latch counts or pin
   counts. Owner's call.
4. **Look at 2c-01 and 2b-02 with one eye on pacing**: a hint or two that split the work into "pages first, then the tree", or a
   second observer test earlier in the stage. Not a restructure.

## 6. Reproducing

```
cp -R courses/bustub/reference /tmp/mut/ref
MUT_ROOT=/tmp/mut/ref MUT_TARGET=/tmp/mut/target MUT_OUT=/tmp/mut/results.jsonl python3 tools/course_mutation.py 6
```

Run it in the background with nothing else building. Never point `MUT_ROOT` at the real reference: a kill in the middle leaves a
mutated file. The first run of this experiment was killed by memory pressure and re-run from a clean copy.
