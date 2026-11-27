# Building Sentinel: a series on its most important parts (backend and parsing, planned, 10 chapters, 96 challenges)

Written 2026-11-27. Planned, not built.

**Keep this private.** Sentinel is proprietary and its repository is private. The anneal repository is public, so this
plan and anything written from it must not go there unless the owner decides how to keep it private.

**Based on** the README, the opening of `docs/PRODUCT.md`, parts of `docs/MVP.md`, decision headings, module sizes, and
the openings and headings of `WORKFLOWS.md`, `ASSET-GRAPH.md`, `INTELLIGENCE.md`, `SURFACE-MODEL.md`, `STORAGE.md`
and `DETECTION.md`.
Sentinel's source was not read (core and web code are about 266,000 lines). Check each phase against the real design
before writing exercises. `docs/sentinel_blueprint.html` wins on any conflict.

## What it teaches

The core: safety limits → collect → understand and save → compare and connect → explain what matters → automate and
learn from feedback. The owner named the core parts: graphs and dynamic programming, asset mapping, a workflow
language that connects tools, the intelligence features, and code diff and change tracking. The owner then limited the
series to **backend and parsing**: no web interface, browser extension, traffic proxy, visual graph editor or active
testing. Backend access control, safety limits and basic API sign-in stay.
Course style: one small project that keeps growing. It is a learning rebuild, not a copy of the product.

## Safety rules for every challenge

- Tests and runs touch only a made-up website on your own machine. No real target, ever.
- The limits are built first and never relaxed: scope required, exclusions win, a rate limit, robots.txt, an audit
  record, a kill switch, secrets removed before storage.
- Passive collection only. No payloads, replay, burst, attack or evasion exercises. A block stays a block.
- Scores and model output only rank and explain. They never send a request.
- Collected data never goes to an outside model or hosted service.
- Outside tools are stand-ins: small made-up programs that print results in a tool-like format.

## Chapters

One growing backend project, used from the command line and a small API with basic sign-in. Each chapter ends with
something you can run. Each challenge has a goal, behaviour tests, hidden hints and one design question. Times are
unmeasured: 45 to 120 minutes. Search appears only where it helps find evidence, code or related assets. Code
understanding uses a parsing library: writing a complete JavaScript parser is not a goal.

**Parsing is mainly in chapters 4 (read input), 5 (understand code) and 8 (build your own language).**

**FlowForge:** chapters 2, 8 and 9 share ideas with FlowForge stages (task graphs, ordering, running work). Do those
FlowForge stages first, and write these challenges so they do not give a FlowForge solution away.

### 1 · Scope and safe collection (challenges 1–10)

- **You can run:** a command that fetches an allowed local test site and refuses everything else.
- **Warm up with:** web server, rate limiter, async runtime.
- **Challenges:** 1 describe a scope · 2 match domains and paths · 3 exclusions and a global deny list · 4 attestation that expires · 5 one gate that every request passes · 6 redirects and name lookups that try to leave the scope · 7 fetch with size and time limits · 8 per-host rate limit and backoff · 9 robots.txt and an identifiable user agent · 10 a kill switch that stops work in flight

### 2 · Jobs and tool runner (challenges 11–18)

- **You can run:** a worker that runs made-up tools and survives a restart.
- **Warm up with:** job service, lease fencing.
- **Challenges:** 11 queue a run · 12 claim with a lease and reject a stale worker · 13 retries with idempotency · 14 shared request budgets that tool requests cannot bypass · 15 run a tool with no shell, bounded output and a time limit · 16 tool manifests that declare what a tool may do · 17 resume after a stop · 18 checkpoint: a fixed two-step collection

### 3 · Evidence and history (challenges 19–26)

- **You can run:** every finding can be traced to when, where and how it was seen.
- **Warm up with:** event ledger, key-value store.
- **Challenges:** 19 store each content once · 20 version history and "as of" a time · 21 an append-only audit log that detects edits, with its limits · 22 remove secrets before storage · 23 where, when and how each fact was seen · 24 "not seen" versus "gone", using what a run covered · 25 recover after a stop at any step · 26 index evidence and search it with filters

### 4 · Parsing foundations (challenges 27–34)

- **You can run:** two made-up tool formats and HTML become one record format.
- **Warm up with:** regex engine, Lox.
- **Challenges:** 27 split input into pieces that keep their positions · 28 a small structured parser with helpful errors · 29 parse a stream that arrives in chunks · 30 size, depth and time limits for hostile input · 31 links and forms from HTML · 32 adapters from two tool output formats into one record · 33 normalise names and addresses so equal things compare equal · 34 fuzz a parser against its own rules

### 5 · Code understanding (challenges 35–42)

- **You can run:** facts about routes and requests that point to a source file and line.
- **Warm up with:** Lox, search engine.
- **Challenges:** 35 walk a JavaScript syntax tree from a parsing library · 36 find links and routes · 37 find requests and their parameters · 38 follow simple constant values, and say when you cannot · 39 source maps back to files and lines · 40 handle broken and minified code without false certainty · 41 index code and facts for search · 42 checkpoint: one site into facts with evidence

### 6 · Code diffs and change tracking (challenges 43–51)

- **You can run:** a command that explains what changed between two versions.
- **Warm up with:** database design foundations.
- **Challenges:** 43 longest common subsequence with dynamic programming · 44 a line diff with a shortest edit script · 45 compare minified code usefully · 46 compare syntax trees · 47 match renamed or moved items with edit distance · 48 facts that appear, disappear and return · 49 baselines · 50 ignore noise without hiding real changes · 51 checkpoint: explain one release's changes

### 7 · Asset mapping and graphs (challenges 52–62)

- **You can run:** a queryable asset map where every link has evidence.
- **Warm up with:** topic series on graphs.
- **Challenges:** 52 things and relationships with evidence · 53 canonical identity · 54 merge sightings from several sources, and undo a wrong merge · 55 conflicting evidence and confidence · 56 neighbourhoods with size limits · 57 shortest evidence path · 58 groups that belong together · 59 what a change can affect · 60 single points that connect groups · 61 the map as of a past time, and comparing two maps · 62 measure on a large made-up graph

### 8 · Workflow language (challenges 63–72)

- **You can run:** a checker that turns pipeline text into a plan you can inspect.
- **Warm up with:** Lox, regex engine.
- **Challenges:** 63 split pipeline text into tokens with positions · 64 parse stages and inputs · 65 recover after an error and report several · 66 a syntax tree you can print back and reparse · 67 resolve names · 68 check that outputs fit the next inputs · 69 reject cycles with a useful explanation · 70 check tool manifests, scope and budgets before running · 71 compile to a plan and show it without running it · 72 fuzz the whole front end

### 9 · Workflow engine (challenges 73–84)

- **You can run:** a pipeline that connects tools and survives restarts.
- **Warm up with:** job service, event ledger, FlowForge stages.
- **Challenges:** 73 run stages in dependency order · 74 spread work over many hosts with a limit · 75 pass results between stages · 76 a failed or empty stage, and what runs next · 77 shared rate and budget limits across stages · 78 cancellation and the kill switch · 79 restart without repeating finished work · 80 old runs keep their pipeline version · 81 stop for a person before continuing · 82 schedules · 83 report what a run covered and missed · 84 checkpoint: simulated crashes at every step

### 10 · Intelligence engine (challenges 85–96)

- **You can run:** a ranked review list with reasons, measured against your own decisions.
- **Warm up with:** search engine.
- **Challenges:** 85 score changes with written rules and reasons · 86 combine facts into a lead and keep the evidence · 87 remove duplicate leads · 88 confirm or reject a lead through the API, with a reason · 89 a fixed labelled set · 90 precision and recall at the top of the list · 91 evaluate in time order without leaking the future · 92 choose what to review within a time budget with dynamic programming · 93 learn weights from decisions, and keep the rules unless it beats them · 94 explain a ranking and say what it does not know · 95 alerts delivered once and signed · 96 final review: test your own limits locally and write what remains unprotected

## First goal

A safe command-line change tracker: chapters 1 and 3 to 6, plus challenges 11 to 13 and 17 from chapter 2 when work
must run in the background. Then chapters 7, 8, 9 and 10.

## Left out

The web interface, browser extension, traffic proxy, visual graph editor, program onboarding, search by meaning, and
anything that actively tests a target.
