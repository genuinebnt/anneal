# anneal

Personal Rust interview-prep platform: axum API (`crates/api`), sandboxed runner (`crates/runner`), file-based
content (`content/tracks`), React SPA (`web`).

**Start with [docs/PROGRESS.md](docs/PROGRESS.md)**, the one live record of what is done, in flight and next (update it at the end of every batch). Then [docs/HANDOFF.md](docs/HANDOFF.md): it covers the current state, how to run and check the work,
the owner's working rules, and the pending work in order. **§6.0 is the pick-up point**: what's unfinished per track,
a checklist of every problem not yet written, and the agent instructions in [docs/authoring/](docs/authoring/).

The rules that matter most:

- Do only what's asked. Propose extras with a recommendation and wait. Never drop existing controls.
- Mock up new screens and big UI changes in HTML and get approval before building them.
- No visible scrollbars. 4-space indent. Don't run prettier (there's no config; match the surrounding style).
- DSA Learn pages and problem tags: the owner fixed them in [docs/DSA_LEARN_PAGE_SPEC.md](docs/DSA_LEARN_PAGE_SPEC.md). Read it before touching `content/dsa/lessons`, `practice.json` or the pattern pages; never deviate.
- Check content with `cargo run -q -p anneal-cli -- verify <track>`, which works without Docker.
- End commit messages with the `Co-Authored-By` line for your model.
- Courses (`courses/`, the Courses pages): read [docs/COURSE_STANDARDS.md](docs/COURSE_STANDARDS.md) first and run `anneal course lint`. Do not rebuild from memory what the owner has already decided.
- Writing, auditing or teaching course content: use the project skills `curriculum-review` and `mentor-teaching` (`.claude/skills/`) and read [docs/CURRICULUM_REASSESSMENT.md](docs/CURRICULUM_REASSESSMENT.md). The BusTub course is being restructured module by module ([docs/BUSTUB_RESTRUCTURE.md](docs/BUSTUB_RESTRUCTURE.md): capability stages, public-API contracts, property tests, optional Rust concepts; module 1a is done); what BusTub does not teach goes to [docs/ADVANCED_DB_COURSE.md](docs/ADVANCED_DB_COURSE.md). Never run `anneal course verify` while editing `courses/bustub/reference` or modules. Long jobs (verify, tests, builds, deploys) run in the background.
