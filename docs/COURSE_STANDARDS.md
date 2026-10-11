# Course standards: what every stage and page must follow

These are the owner's decisions, in their own words where it matters. They were given over several sessions because earlier work drifted from
them. **Read this before changing anything under `courses/` or the Courses pages (`web/src/pages/Course*`)**, and run `anneal course lint`
(it also runs in CI). Do not rely on memory of "what CodeCrafters looks like": fetch or ask when a site is named as the model.

## The decisions

1. **A learning track as well as a project track.** The goal is to teach an idea and have the learner use it to solve the exercise. Every
   stage names what it teaches (`learn = [...]` in `stage.toml`), links at least two concept articles (`concepts = [...]`, files in `courses/<id>/concepts/`; articles are shared between stages and modules),
   and the exercise uses that concept. The concept comes first on the page ("Read first").
2. **Stage size.** *"not too small but self contained exercises"*, *"i am not looking for one or two line change exercise"*, *"6 - 10 exercises
   is sweetspot but not a hard requirement"*. A stage is a self-contained exercise: tens of lines and a design decision, passable on its own.
   Group tiny pieces; split a chapter. About 6 to 10 stages per module; the last stage of a module is BusTub's own test, ported (kind `boss`).
3. **Tests.** *"have atleast 5 test cases"* and *"ensure the texts dont clutter the challenges with too many unnecessary tests"*. Every
   non-boss stage has at least 5 tests, each checking a distinct behaviour (no padding, no near-duplicates). The stage text summarises the key
   behaviours: at most 4 bullets per `### Tests` block and 8 per stage; the learner sees every test by name in the Last run tab.
4. **Performance.** *"have performance sections"*. Every non-boss stage has a `## Performance` section: the cost model (complexity, syscalls,
   allocations, lock hold times), what the obvious version costs, what to measure and how (`strace -c`, a timing test, counters). Claims must be
   things you can check; hedge ("typically") instead of inventing numbers.
5. **Hints at BusTub's level.** *"the hints and the content should not be trivial or basic … bustub is a serious exercise"*. `## Hints` has at
   least 2 `###` hints per stage, deepest last: the design choice, then the trap, then the invariant to check. Never a restatement of the task.
6. **Pages.** The course overview is sections (modules) of stage rows with a difficulty mark, like CodeCrafters' course page; each stage is its own
   page with tabs (Instructions, Hints, Solution, Concepts, Last run); a concept is its own page. Everything uses anneal's design (the /rust
   catalog's styles and components), with callouts, highlighted code and colour-coded C/C++ vs Rust tables. Mockups come before big UI changes
   (CLAUDE.md) and are built from the real reference, not from memory.
7. **Explain with pictures where they help.** *"you can show svg visualizations animations tables graphs diagrams etc to explain things"*. Concept
   articles and stage pages take inline SVG (an ```svg fence, optionally starting with `caption: ...`), CSS animations, tables and graphs. They are
   drawn with the app's own classes in `web/src/styles/course.css` (`box`, `live`, `free`, `hot`, `blue`, `ln`, `flow`, `pulse`, ...) so they follow
   the theme; animations must respect `prefers-reduced-motion` (the stylesheet turns them off). Draw what is hard to see in prose: layouts,
   state machines, timelines, cost curves. Module 1a's four concept articles are the examples.
8. **Mirror BusTub; show the C/C++ way** (tables, "Port rule" callouts), link CMU lectures and verified resources, keep the learner free to
   solve it their own way (only the tests must pass), test every stage (fail before, pass after) and ship module by module.

9. **Hide what the learner hasn't reached.** *"hide the modules not completed from mod.rs files"*. A learner's repo contains only the modules
   up to the one they are on: later modules' source files and tests are absent, and `mod.rs`/`lib.rs` don't name them, so `cargo test` compiles
   and runs only what is unlocked. Passing the last stage of a module brings in the next one (`anneal course test`, `next` or `update`). The
   file → module map is `template/.anneal-files.json` (written by `anneal course template`); a module that must ship a file a later module
   also touches lists it under `files = [...]` in `module.toml`. `anneal course verify` compiles every unlock state, and the smoke test
   plays through module 1a and checks that 1b arrives (`crates/cli/src/course_unlock.rs`).

10. **Concepts teach code the learner can use.** *"concepts teach code. if we are talking about a datastructure or a struct then showcase its
   methods, functions, way to use them, common usecase in real code ... if its an algorithm explain it and show how to use it and where its used
   ... i am using concepts to fill gap between what i know and what i need to know to solve each exercise, plus additional learning that will
   be useful"*. Every concept linked from a stage ends with `## In real code`: (a) the API or algorithm in use, as complete runnable code with
   a worked trace, in two or more ```` ```rust test ```` examples (`#[test]` functions that `python3 tools/course_snippets.py` compiles and
   runs, and CI runs); (b) `### In the exercises`: which stage (by id and Part) uses which tool, and how; (c) `### Where it is used`: real
   systems and libraries, stated only when sure. Stage ids in (b) are checked against the course by hand: use the ids in `stage.toml`.

11. **The learner thinks; the course removes friction, not thinking.** *"i should not feel this is useless and i just do bustub the original way … i learn a lot and build and experiment and use my brain a lot but also move away unnecessary friction"* (2026-10-09). Friction that teaches nothing goes (setup, plumbing, unclear tasks, slow or vague feedback, hunting for context). Design, debugging and experiments stay with the learner. In practice:
    - **A task is a contract, not a recipe.** It states the observable behaviour, the invariants and the file. A step-by-step recipe, if it helps, goes in a collapsed `> [!ASIDE] The steps, if you would rather not work them out` (see 2c-04), never as numbered steps in the task. Hints say *why*, not the line of code.
    - **Leave design open where the idea is the lesson.** "Given" scaffolding is for plumbing. A stage whose point is a data structure or an algorithm says "the fields are a suggestion; the tests use only the public methods" (see 1c-01).
    - **Open check-yourself questions, never multiple choice.** `> [!CHECK]` asks the learner to predict, trace or explain before coding; the answer is hidden and three nudges open one by one. About one per stage where a concept needs it, always one that needs reasoning ("which slots does each new page get, and does the file grow?").
    - **Experiments.** A stage that only runs BusTub's own test (`Nothing new to write`) ends with an optional `## Experiment`: predict, change one thing, measure or break it, and explain. The learner writes the prediction down first.
    - **Tests explain themselves.** Every assertion in a stage test names the behaviour it checks (`tools/add_assert_messages.py`; `--check` lists bare ones): a failure says what broke, not just `assertion failed: ok`.
    - **Concept links say how much.** `concepts` are what the stage needs; `concepts_optional` are further reading that names the stage. A concept that lists a stage under "In the exercises" is linked from that stage.

## How these are enforced

- `anneal course lint` (content only, needs no reference): for every non-boss stage of a *published* module (`published_modules` in `course.toml`;
  `--all` checks the rest): `learn` has 2 or more entries, at least two concepts are linked, a Performance section exists, at least 2 hints, and the
  `### Tests` text stays short (at most 4 bullets per block, 8 per stage); every linked concept has an `In real code` section with two or
  more `#[test]` examples, `### In the exercises` and `### Where it is used`. CI runs it, and runs `tools/course_snippets.py` on the examples.
- `anneal course verify` also fails a non-boss stage with fewer than 5 tests.
- A module is added to `published_modules` only when lint and verify are clean for it.

## Code comments (owner, 2026-10-11)

The `///` and `//!` comments in the template and reference stay in the learner's code after the course, so they are written for a
reader who has never seen the course, in the style of the Rust standard library:

- **One summary sentence**, third person, present tense ("Returns the number of ...", "Removes ...", "A queue that ..."), then only
  what a caller needs: the **behaviour** at the edges, the **invariants** that always hold, and `# Errors` / `# Panics` sections.
- **Fewer words, more meaning.** No steps, no "first ... then", no implementation (which map, which loop). If the sentence says how, it
  is the wrong sentence.
- **Nothing course-specific:** no stage or module ids, no "module 4", "this exercise", "the tests", "yours", "given code", no hints.
- **Nothing historical:** no "BusTub does ...", no "unlike C++", no "this course leaves out ...", no "port of ...".
- Every item a learner implements has one. What to do *now* belongs in the `todo!("<stage>: ...")` message, which the learner replaces;
  it states the behaviour wanted, not the steps.

`tools/check_code_comments.py` lists breaches (course words, history, a stub without a doc); keep it at zero.
