# FlowForge series plan

A plan only. Nothing here changes a stage, a test, `./ff`, `ROADMAP.md` or `CLAUDE.md`. Time boxes are the mentor's
estimates (45, 60, 90 or 120 minutes of focused work), not measurements. Part ids (`W09`, `N03`) are plan ids. Only
stages 01-08 exist as task and test files.

## Start here

**How to use it.** Follow the **main path** in order; it always says what comes next. **Branches** are optional: when
one opens, take two or three of its parts between main-path stages, one branch at a time, then come back. Every part
has one home series, adds one main new idea, names what it reuses, where it returns, and where it is useful outside
FlowForge.

**Reading the links.** *Required first* means that feature must already exist in your FlowForge (at most two direct
ones; their own requirements follow). *Helpful* is optional knowledge. *Used again in* is the reverse of both. Some
links would give away an approach you should find yourself; they are only in Appendix A. **Do not read the
appendices before finishing the part concerned.**

**Horizons.** `now` = the 8 written stages and the six ladder items (sequential run, concurrent run with limit N,
critical path, persistence and restart, REST API, CLI client) plus what they strictly need. `next` = deepening that
builds directly on `now`. `far` = later; do not start it early because it looks interesting. A `now` part never
requires a `next` or `far` one, and `next` never requires `far`.

### Your next 10 steps

The 8 stages are written and `src/main.rs` is still the placeholder, so you start at step 1.

| Step | Part | Time | The one new idea |
|---|---|---|---|
| 1 | W01 Load and count | 45 min | a typed domain model for a definition |
| 2 | W02 Fail cleanly | 45 min | errors as values |
| 3 | W03 The schema is a contract | 45 min | parse, don't validate |
| 4 | W04 Meaningful definitions | 45 min | semantic validation |
| 5 | W05 No cycles | 90 min | the DAG invariant |
| 6 | W06 A safe order | 45 min | ordering as a planning output |
| 7 | W07 Same answer every time | 45 min | determinism and invariant testing |
| 8 | W08 Works at size | 90 min | measure before optimising |
| 9 | C01 Sequential run | 90 min | async execution |
| 10 | TR01 Check my trace | 45 min | invariants as an executable oracle |

### The main path (68 steps, about 84 hours)

`TR01` at step 10 is a skippable companion: nothing in `now` requires it.

**Stage 1. Foundations (the 8 written stages)** (`now`; 7.5 h)  
1.W01 Load and count → 2.W02 Fail cleanly → 3.W03 The schema is a contract → 4.W04 Meaningful definitions → 5.W05 No cycles → 6.W06 A safe order → 7.W07 Same answer every time → 8.W08 Works at size

**Stage 2. The ladder: run, remember, serve** (`now`, with companion parts; 14.75 h)  
9.C01 Sequential run → 10.TR01 Check my trace → 11.C02 Run independent tasks together → 12.C03 Never more than N → 13.S01 Critical path → 14.D01 Run history → 15.D02 All of a change, or none → 16.D03 Restart recovery → 17.A01 Definitions over HTTP → 18.A02 Runs over HTTP → 19.A03 CLI client

**Stage 3. Revisit before going further** (`next`; 3.75 h)  
20.AG01 What does this change touch? → 21.O01 The engine is a library → 22.O02 Errors callers can act on → 23.AD01 Did you mean?

**Stage 4. Runs that fail and time that can be controlled** (`next`; 12.75 h)  
24.W09 When a task fails → 25.C04 Virtual time and seeded runs → 26.W10 Try again → 27.W11 Wait before trying again → 28.W12 Timeouts → 29.C05 One writer, many producers → 30.C06 Stop what is running → 31.C07 Leave cleanly → 32.C08 Cancellation safety → 33.W13 Cancel a workflow → 34.AG02 Only what I need

**Stage 5. Choosing what runs** (`next`; 8.75 h)  
35.S02 Why is it waiting → 36.S03 Priorities → 37.AD02 How much slack? → 38.S05 Resource limits → 39.AB01 Which workers can take it? → 40.S07 Delayed starts → 41.AI01 Blackout windows → 42.AI02 When can it start? → 43.AY01 The most bookings

**Stage 6. Queues** (`next`; 15.25 h)  
44.Q01 The ready queue made visible → 45.Q02 Bounded with backpressure → 46.Y01 Logs you can query → 47.Y02 Numbers that add up → 48.Q04 Delay queue → 49.Q05 A queue that survives → 50.Q06 At least once → 51.Q07 Leases and visibility timeouts → 52.Q08 Idempotent effects → 53.TR04 Fail on purpose → 54.Q09 Dead letters

**Stage 7. Many processes, outward events** (`next`; 9.75 h)  
55.T01 A wire protocol → 56.Q11 Separate worker processes → 57.X01 Transactional outbox → 58.X02 Idempotent consumers → 59.O04 Add a task kind → 60.B01 Run a real command → 61.M01 Do not prepare twice

**Stage 8. Distributed foundations (simulator first)** (`far`; 11 h)  
62.N01 Two processes, only messages → 63.N02 An unreliable network → 64.N03 No answer is not "no" → 65.N04 Delivery guarantees end to end → 66.N05 Is it dead? → 67.N07 Fencing → 68.N08 Real remote workers

### Branch menu

"Opens after" is the main step after which the branch's **first** part has what it needs from the main path.
"Also needs" lists parts from other branches required somewhere along it. Later parts may need later main steps;
each part's own *Required first* decides.

| Branch (remaining parts) | First part | Opens after step | Also needs from other branches |
|---|---|---|---|
| R CRDT: replicas and merging (17) | R01 | 4 | - |
| Z Security (8) | Z01 | 8 | A04, J03, T04 |
| G Parsers and compilers (11) | G01 | 8 | W14 |
| K Storage engine (12) | K01 | 8 | - |
| AR Recursion and backtracking (6) | AR01 | 8 | - |
| AG Graphs (8) | AG03 | 8 | W16 |
| AD Dynamic programming (5) | AD03 | 8 | S04 |
| TR Testing and reliability (5) | TR02 | 10 | K02 |
| C Concurrency (3) | C09 | 12 | - |
| P Performance (6) | P01 | 12 | C10, C11 |
| I Operating systems (5) | I01 | 14 | B02, K02, T02 |
| J Lineage and event history (6) | J01 | 14 | E01, K03 |
| AI Intervals (4) | AI03 | 14 | - |
| E Databases: query and execution (6) | E01 | 16 | G02, P03, V16 |
| H Search (8) | H01 | 16 | K02 |
| V Vectors and discovery (17) | V01 | 16 (after H01) | H01, H05, J02, K09, P06 |
| A Networking and web: API and CLI (8) | A04 | 18 | W18 |
| AB Bit manipulation (4) | AB02 | 18 (after A04, Z05, Z06) | AD07, Z06 |
| O Ownership and API design (4) | O03 | 21 | C10, P01 |
| W Workflows (8) | W14 | 24 | J03 |
| X Delivery and operations (4) | X03 | 31 | - |
| S Scheduling (7) | S04 | 36 | C11 |
| AY Greedy (4) | AY02 | 41 | AD07 |
| MX Combinations (6) | MX01 | 42 (after S04) | AB05, AD04, AI03, AR06, AY03, M04, S04 |
| Q Queues (2) | Q03 | 45 | - |
| L Rate limiting and overload (11) | L01 | 45 (after Q10) | Q10, S06 |
| Y Monitoring and debugging (5) | Y03 | 46 | Q10, TR03 |
| T Networking: wire and protocols (5) | T02 | 55 | - |
| B Task sandboxing (5) | B02 | 60 | Z03, Z07 |
| M Caches and memory (5) | M02 | 61 | P01 |
| N Distributed systems (16) | N06 | 66 | K02, K03, L05, TR07, W19 |

Suggested first branches, so that practice is spaced rather than random:

| After main stage | Good branch parts to take | Why then |
|---|---|---|
| 1 | AG03, AG07, AR01, AR02, AD03 | revisit the dependency model in new questions while it is fresh |
| 2 | TR03, M02, Z02, Z03, I01 | harden what you just built |
| 4 | R01, R02, W15, C11 | controlled time makes convergence and interleaving work testable |
| 5 | AI03, AI04, AY03, AB03, S04, MX01 | return to scheduling and calendars from other angles |
| 6 | L01, L03, R03 to R05, J01 | queue guarantees reappear as limits and merges |
| 8 | one of: N06 onwards (consensus), R06 to R17 (replicas), G (language), K and E (databases), H and V (search) | choose one long track |

## Concept map

Concept, where it is first taught, where you deliberately meet it again in a different setting, and where it applies
elsewhere. `(step n)` means main path; the others are on branches. Algorithm-family concepts are in Appendix A so
that this table gives nothing away.

| Concept | First taught | Practised again | Applied outside FlowForge |
|---|---|---|---|
| Typed models of untrusted input | W01 (step 1) | W03 (step 3), W04 (step 4), A01 (step 17), Z02, O02 (step 22) | config loaders, API bodies |
| Errors as values and error contracts | W02 (step 2) | W04 (step 4), A01 (step 17), O02 (step 22), N03 (step 64), Q09 (step 54) | every library and service |
| Invariants checked over many generated inputs | W07 (step 7) | C01 (step 9), TR01 (step 10), TR03, C04 (step 25), R02, N13 | testing any system |
| Determinism: same input, same decisions | W07 (step 7) | C04 (step 25), M04, R02, N01 (step 62), J03 | reproducible builds, replay |
| Measure before optimising | W08 (step 8) | P01, AB03, M05, Y07, V03 | all performance work |
| Dependency structure and what it implies | W05 (step 5) | W06 (step 6), AG01 (step 20), AG02 (step 34), J02, R09 | build systems, package managers |
| Event trace as the observable contract | C01 (step 9) | TR01 (step 10), C03 (step 12), D01 (step 14), A07, Y03, N13 | logs, audits, debugging |
| Futures and async execution | C01 (step 9) | C02 (step 11), C06 (step 30), C08 (step 32), A01 (step 17), T01 (step 55) | async services |
| Bounding concurrent work | C03 (step 12) | S05 (step 38), Q02 (step 45), N03 (step 64), L07 | pools, crawlers |
| Time as an input | C04 (step 25) | W12 (step 28), S07 (step 40), Q04 (step 48), Q07 (step 51), N03 (step 64), L01 | schedulers, limiters, caches |
| Cancellation and shutdown | C06 (step 30) | C07 (step 31), C08 (step 32), W13 (step 33), A06, B02, T02, X03 | every server |
| Channels and single ownership | C05 (step 29) | Q02 (step 45), A07, O05, T01 (step 55) | pipelines |
| Shared state and consistent snapshots | C10 | O05, P03, K10 | dashboards, databases |
| Lifecycles with legal transitions | W09 (step 24) | W13 (step 33), Q06 (step 50), R16 | orders, payments |
| Retrying safely | W10 (step 26) | W11 (step 27), Q06 (step 50), Q08 (step 52), T04, L09, N04 (step 65) | every client |
| Idempotency | Q08 (step 52) | A05, X02 (step 58), N04 (step 65), N17, T04 | payments, consumers |
| Delivery guarantees | Q06 (step 50) | Q07 (step 51), X01 (step 57), X02 (step 58), T05, N04 (step 65) | message brokers |
| Backpressure and admission | Q02 (step 45) | Q10, T02, L01, L11 | overload protection |
| Ownership of work over time (leases) | Q07 (step 51) | N07 (step 67), N09, N14 | distributed locks |
| Durability and crash recovery | D01 (step 14) | D03 (step 16), Q05 (step 49), X01 (step 57), TR04 (step 53), K02, K04, N16 | databases, job systems |
| Public contracts and their evolution | A01 (step 17) | A04, W18, TR05, O07, A11 | APIs and file formats |
| Observability | Y01 (step 46) | Y02 (step 47), Y03, Y04, Y05, Y06 | operating anything |
| Traits and extension points | O04 (step 59) | O05, T04, B01 (step 60), T06 | plugins, middleware |
| Library boundaries | O01 (step 21) | O02 (step 22), O03, O07 | any crate |
| Process and network boundaries | T01 (step 55) | Q11 (step 56), N01 (step 62), N08 (step 68), B01 (step 60), I03 | distributed software |
| Explicit fault models and simulation | N01 (step 62) | N02 (step 63), N05 (step 66), N13, N23, R02 | distributed testing |
| Convergence without coordination | R02 | R03, R04, R08, R10, R16, R17 | local-first and sync |
| Causality and logical time | R06 | R07, N10, R11, R13 | sync engines, logs |
| Agreement among replicas | N11 | N12, N13, N14, N15, N20 | databases, control planes |
| Caching and invalidation | M01 (step 61) | M02, M03, M04, MX02 | web and build caching |
| Hostile input and trust boundaries | Z02 | Z03, Z04, B05, Z07 | security work |
| Explaining a result to the user | W04 (step 4) | S02 (step 35), AG04, AG05, M04, MX02, G04 | linters, planners |
| Exact answers as oracles for faster ones | W07 (step 7) | AD04, AY03, AD07, MX06, V03 | verifying any heuristic |

## Capstones

Every series ends in a FlowForge milestone, split into sittings of 45 to 120 minutes that each end in tested
behaviour. A capstone adds no new concept. Its **readiness table** names the earlier part for each thing it needs; do
not start until every listed part is done. Milestones follow each other in order, and their readiness table replaces
the two-link limit that ordinary parts have. Nothing requires a capstone, so capstones cannot create cycles. Setup work
that `CLAUDE.md` says to explain directly (crates, database, cloud, CI) is given by the mentor and is not listed as a
concept. Gaps found while building these tables became preparation parts: D02 transactions, K01 trustworthy disk
records, V01 logs to vectors, H04 what counts as a word, Z01 threat model, P02 reading a profile, P04 unsafe
boundaries, TR02 input generators.

## The series

| Id | Series | Group | Parts | now / next / far | Hours |
|---|---|---|---|---|---|
| W | Workflows | core | 21 | 8 / 10 / 3 | 25 |
| C | Concurrency | core | 11 | 3 / 8 / 0 | 16 |
| S | Scheduling | core | 12 | 1 / 7 / 4 | 19 |
| Q | Queues | core | 11 | 0 / 11 / 0 | 15 |
| D | Durable state | platform | 3 | 3 / 0 / 0 | 4 |
| A | Networking and web: API and CLI | platform | 11 | 3 / 8 / 0 | 12 |
| O | Ownership and API design | subject | 7 | 0 / 5 / 2 | 9 |
| TR | Testing and reliability | subject | 7 | 0 / 5 / 2 | 9 |
| Y | Monitoring and debugging | subject | 7 | 0 / 6 / 1 | 8 |
| X | Delivery and operations | platform | 6 | 0 / 6 / 0 | 8 |
| T | Networking: wire and protocols | subject | 6 | 0 / 3 / 3 | 8 |
| B | Task sandboxing | subject | 6 | 0 / 2 / 4 | 9 |
| I | Operating systems | subject | 5 | 0 / 1 / 4 | 7 |
| M | Caches and memory | subject | 6 | 0 / 4 / 2 | 8 |
| Z | Security | subject | 8 | 0 / 6 / 2 | 9 |
| J | Lineage and event history | subject | 6 | 0 / 0 / 6 | 9 |
| N | Distributed systems | progression | 23 | 0 / 0 / 23 | 38 |
| R | CRDT: replicas and merging | progression | 17 | 0 / 0 / 17 | 25 |
| L | Rate limiting and overload | subject | 11 | 0 / 0 / 11 | 12 |
| G | Parsers and compilers | subject | 11 | 0 / 0 / 11 | 19 |
| K | Storage engine | subject | 12 | 0 / 0 / 12 | 19 |
| E | Databases: query and execution | subject | 6 | 0 / 0 / 6 | 10 |
| H | Search | subject | 8 | 0 / 0 / 8 | 11 |
| V | Vectors and discovery | subject | 17 | 0 / 0 / 17 | 24 |
| P | Performance | subject | 6 | 0 / 0 / 6 | 8 |
| AR | Recursion and backtracking | algorithm | 6 | 0 / 5 / 1 | 8 |
| AG | Graphs | algorithm | 10 | 0 / 7 / 3 | 14 |
| AD | Dynamic programming | algorithm | 7 | 0 / 4 / 3 | 11 |
| AI | Intervals | algorithm | 6 | 0 / 4 / 2 | 8 |
| AY | Greedy | algorithm | 5 | 0 / 3 / 2 | 5 |
| AB | Bit manipulation | algorithm | 5 | 0 / 2 / 3 | 6 |
| MX | Combinations | combination | 6 | 0 / 2 / 4 | 11 |
| | **Total** | | **289** | **18 / 109 / 162** | **403** |

**Core** series each teach one subject close to FlowForge's own domain. **Subject** and **platform** series do the
same for one supporting area. **Progressions** (N, R) are long ladders. **Algorithm** series give repeated practice
in one family. **Combinations** mix ideas already practised and add no new concept. The total is a long-term map, not
a commitment.

Where the requested areas live: ownership and API design O; parsers and compilers G; storage K; databases E (with
K10 to K12 for snapshots and concurrent writers); caches and memory M; networking and web A and T; search H;
monitoring and debugging Y; testing and reliability TR; performance P; security Z; task sandboxing B; operating
systems I; lineage and event history J; rate limiting L and N22; vectors V; scheduler algorithms S.

## Testing and monitoring alongside the main stages

These ride along with a main stage. The mentor writes the stage tests; the owner builds the product affordance.

| While you do | Testing companion | Monitoring companion |
|---|---|---|
| C01 to C03 run | TR01 check a trace | the trace itself; Y01 once there is a server |
| D01 to D03 persistence | TR04 fault points (later on the path; may be pulled forward) | Y04 time attribution (after S01) |
| A01 to A03 API and CLI | TR05 API contract | Y01 logs |
| C04 to C07 time and shutdown | C04 virtual time, C11 interleavings | X03 ready, alive, draining |
| Q01 to Q09 queues | TR03 shrinking | Y02 metrics, Y05 stall detection |
| Q11, N parts | TR07 chaos, N13 history checker | Y03 request traces, Y06 debug bundle, Y07 load |

## Testing contract for all parts

Black box only, as `CLAUDE.md` requires: the compiled binary, its files, sockets and exit codes. These are proposals
for contracts to fix when the first part that needs each is written.

1. **Run trace** (from C01): a machine-readable line per observable event. Nearly every later test asserts invariants
   over it. Order of events, never measured durations.
2. **Hold-open tasks** (from C02): a test-controlled task kind that finishes when the test says, so overlap is forced
   rather than hoped for.
3. **Virtual time and seeds** (from C04): time-based behaviour is checked on a simulated clock and replayed by seed.
4. **Named fault points** (D03, TR04): "stop after the n-th committed change", swept over every n.
5. **Simulator** (from N01, R02): network and delivery faults come from a seed or script.
6. **Work counters instead of timings** for efficiency claims.
7. Servers listen on port 0 and print the address. External services are opt-in and outside the default suite.

Adding task fields (priority, resources, scripted failure) changes the schema contract decided in stage 03. Decide per
part whether it is an optional workflow field or a separate scenario file. `ROADMAP.md` suggests in-process hooks and
`src/lib.rs` for stage 10; items 1-3 are an alternative that keeps tests from importing internal modules.

## The 8 existing stages, placed honestly

Estimates assume a first pass by someone learning Rust, excluding optional reading and Rust craft exercises. No
existing task or test file is to be renamed, renumbered or edited; "merge" means sitting two stages in one session.

| Stage | Tests | Estimate | Fit | Proposal (without touching files) |
|---|---|---|---|---|
| 01 hello (W01) | 2 | 45 min | fits | includes first dependency setup; keep alone |
| 02 bad-input (W02) | 4 | 45 min | fits | keep alone |
| 03 shape (W03) | 4 | 15-45 min | **small**: its own Hint 3 says there is nothing more to add, and parts of it may already pass after stages 1-2 | do 03 and 04 as one 60 min session; or fill 03 with its design questions (newtype id, unknown fields) |
| 04 references (W04) | 4 | 45 min | low end | see above |
| 05 cycles (W05) | 6 | 90 min, up to 120 | fits, hardest of the first eight | if over 120: session A representation and small cases, session B ring and randomized tests |
| 06 order-basics (W06) | 5 | 20-45 min | **small** if 05 went well | one 60-90 min session with 07 |
| 07 order-invariants (W07) | 4 | 45 min | fits | see above; add the shrinking lesson to fill a longer box |
| 08 scale (W08) | 4 | 45-120 min | **variable**: tests may pass at once if 05-07 were efficient, leaving mostly measurement | box at 90 and require a before/after profile note even if green; split off profiling if it overruns |

Other observations. Parts W01-W08 keep their stages' contents even where a stage teaches more than one idea; the
one-idea rule is applied to new parts. Stage 08 uses wall-clock limits (5 s and 10 s); they are generous, but they are
the one place that departs from "no timing-flaky checks", so later parts use work counters. Stage 06 mentions "stage
9" and stages 02-03 mention "stage 15"; see the runner section for numbering. Possible contract questions for later
edge-case tests are held back until you have a first pass, as `CLAUDE.md` says.

## How the core four feed each other

Each cell lists direct links `earlier → later`: the row series' part is reused by the column series' part
(Required or Helpful).

| from \ to | Workflows | Concurrency | Scheduling | Queues |
|---|---|---|---|---|
| **Workflows** | within series | W07→C01, W06→C01 | W08→S01 | W11→Q04, W10→Q08, W10→Q09 |
| **Concurrency** | C03→W09, C04→W10, C04→W12, C06→W13 | within series | C03→S02, C04→S03, C04→S07, C11→S12 | C03→Q01, C05→Q02, C04→Q04, C04→Q07 |
| **Scheduling** | - | - | within series | S03→Q03, S07→Q04 |
| **Queues** | - | - | Q11→S09 | within series |

## What the distributed (N) and CRDT (R) progressions need from the core

"Direct" is a Required or Helpful link into Workflows, Queues, Scheduling or Concurrency. "All required" follows the
Required chain and omits stages 01-08 (W01-W08). A dash means only earlier parts of the same or another series.

| Part | Direct core links | All required core parts beyond stage 08 |
|---|---|---|
| N01 Two processes, only messages | C04, Q07 | C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07 |
| N02 An unreliable network | - | C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07 |
| N03 No answer is not "no" | W11 | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07 |
| N04 Delivery guarantees end to end | Q08 | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N05 Is it dead? | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07 |
| N06 Everyone learns who is alive | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07 |
| N07 Fencing | Q07 (helpful) | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N08 Real remote workers | Q11 | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08, Q09, Q11 |
| N09 Partitioned scheduling | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N10 Clocks that disagree | - | C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07 |
| N11 Copies of run state | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N12 How many copies must answer | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N13 Was that history legal? | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N14 Raft 1: one leader | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N15 Raft 2: one log | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N16 Raft 3: crashes and disks | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N17 Raft 4: clients | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N18 Raft 5: bounded logs | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N19 Raft 6: new members | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N20 All partitions or none | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N21 Undo instead of lock | W19 | W09, W10, W11, W13, W19, C01, C02, C03, C04, C05, C06, Q01, Q02, Q04, Q05, Q06, Q07, Q08 |
| N22 One limit, many nodes | - | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08, Q10 |
| N23 Break it on purpose | Q07 (helpful) | W09, W10, W11, C01, C02, C03, C04, C05, Q01, Q02, Q04, Q05, Q06, Q07, Q08, Q09, Q11 |
| R01 Two copies, two answers | W04 | - |
| R02 Convergence you can test | C04 | C01, C02, C03, C04 |
| R03 Counting while apart | - | C01, C02, C03, C04 |
| R04 Add and remove while apart | - | C01, C02, C03, C04 |
| R05 One value, two writers | - | C01, C02, C03, C04 |
| R06 Which came after which | - | C01, C02, C03, C04 |
| R07 Newer, or just different? | - | C01, C02, C03, C04 |
| R08 Maps of mergeable things | - | C01, C02, C03, C04 |
| R09 Converged but invalid | W05 | C01, C02, C03, C04 |
| R10 Send the change, not the state | - | C01, C02, C03, C04 |
| R11 Send only what is missing | - | C01, C02, C03, C04 |
| R12 What deletion costs | - | C01, C02, C03, C04 |
| R13 Forgetting safely | - | C01, C02, C03, C04 |
| R14 An ordered list of steps | - | C01, C02, C03, C04 |
| R15 Typing at the same time | - | C01, C02, C03, C04 |
| R16 Reports from offline workers | W09 | W09, C01, C02, C03, C04 |
| R17 Edit one workflow together | - | C01, C02, C03, C04 |

## Dependency roadmap

| Series | Directly requires parts from |
|---|---|
| W | A, C, D, J |
| C | W |
| S | C, D, Q, W |
| Q | C, D, S, T, W |
| D | C |
| A | C, D, Q, W |
| O | C, P, W |
| TR | A, C, D, K, O, Q, W |
| Y | A, C, D, Q, S, TR |
| X | A, C, D, Q, Y |
| T | C, O, Q, W, X |
| B | O, S, W, Z |
| I | B, D, K, Q, T, TR |
| M | A, C, D, P, W |
| Z | A, D, J, T, W, Y |
| J | AG, D, E, K, W |
| N | C, D, K, L, Q, TR, W |
| R | C, W |
| L | C, Q, S, W |
| G | W |
| K | D, W |
| E | D, G, P, V |
| H | D, K |
| V | D, H, J, K, P, W |
| P | C, W |
| AR | S, W |
| AG | C, W |
| AD | D, S, W |
| AI | A, D, S, W |
| AY | AD, AI, D, S |
| AB | AD, AG, D, S, W, Z |
| MX | AB, AD, AG, AI, AR, AY, M, S |

**Cycle check** (script run when this file was generated, over all 289 parts):

- the Required graph between parts has no cycle; adding Helpful links, and then the Appendix A links, still gives none;
- no part has more than two direct Required links;
- every main-path part's Required parts come earlier on the path, and no Helpful link on the path points forward;
- every `now` part is on the main path; horizons never require a later horizon;
- within a series, no part requires a higher-numbered one;
- concept map: no "practised again" part is required by the part that first teaches the concept, or precedes it on the path;
- longest Required chain: 28 parts.

**Honest limit:** collapsed to whole series the graph is *not* acyclic, because series are meant to feed each other.
Mutual pairs: A and W, C and W, J and W, Q and S, Q and T. The orderable units are parts and the main-path stages, not whole series, so never read "finish
series X first".

## The parts

Each entry: time, horizon, position; the idea it **adds**; the behaviour (**goal**); links; use **outside**; how a
black-box **test** would check it; the Rust **craft** topic. Repositories are named as reading candidates only: none
was read for this plan, so read before teaching from one. Entries "by name" are known only by name. **Cookbook**:
`related` = a neighbouring technique; `similar` = does the part's core idea in another domain, so open it only after
your own first pass. Cookbook entries are under `cookbook/examples/`; their title lines were read (headers for many, a
few bodies in part). They are references, never stage solutions. In algorithm and combination parts the new idea is
deliberately blank and cookbook pointers are in Appendix B.

### W · Workflows (core)

What a workflow is and what it means to run one: definitions, validation, ordering, failure, retries, timeouts, conditions, fan-out and fan-in, cancellation, resumption, versions, dynamic workflows.

#### W01 · Load and count
45 min · `now` · main step 1
- **Adds:** a typed domain model for a definition
- **Goal:** `validate` reads a definition and reports its task count (stage 01)
- **Required first:** nothing
- **Used again in:** W02
- **Outside FlowForge:** every program that reads config or data files
- **Test:** existing `stage_01_hello`
- **Craft:** separating domain types from I/O (ripgrep)

#### W02 · Fail cleanly
45 min · `now` · main step 2
- **Adds:** errors as values
- **Goal:** bad input gives a message and exit 1, never a crash (stage 02)
- **Required first:** W01 (a typed domain model for a definition)
- **Used again in:** W03
- **Outside FlowForge:** every CLI and service
- **Test:** existing `stage_02_bad_input`
- **Craft:** error design (ripgrep, thiserror, anyhow)

#### W03 · The schema is a contract
45 min · `now` · main step 3
- **Adds:** parse, don't validate
- **Goal:** only documents matching the schema are accepted (stage 03)
- **Required first:** W02 (errors as values)
- **Used again in:** W04, AI01
- **Outside FlowForge:** API request bodies, config loading
- **Test:** existing `stage_03_shape`
- **Craft:** newtypes, strictness (serde)

#### W04 · Meaningful definitions
45 min · `now` · main step 4
- **Adds:** semantic validation
- **Goal:** unknown dependencies and duplicate ids are named (stage 04)
- **Required first:** W03 (parse, don't validate)
- **Used again in:** W05, J01, R01, AD01, AD04
- **Outside FlowForge:** referential checks in any import job
- **Test:** existing `stage_04_references`
- **Craft:** errors that carry data (thiserror, regex)

#### W05 · No cycles
90 min · `now` · main step 5
- **Adds:** the DAG invariant
- **Goal:** cyclic definitions are rejected (stage 05)
- **Required first:** W04 (semantic validation)
- **Used again in:** W06, R09
- **Outside FlowForge:** build systems, package managers, spreadsheet formulas
- **Test:** existing `stage_05_cycles`
- **Craft:** graph representation and typed indices (petgraph)

#### W06 · A safe order
45 min · `now` · main step 6
- **Adds:** ordering as a planning output
- **Goal:** `order` prints an executable order (stage 06)
- **Required first:** W05 (the DAG invariant)
- **Used again in:** W07, C01
- **Outside FlowForge:** installers, migration runners
- **Test:** existing `stage_06_order_basics`
- **Craft:** pure core, effectful shell (ripgrep)

#### W07 · Same answer every time
45 min · `now` · main step 7
- **Adds:** determinism and invariant testing
- **Goal:** the order is correct on random DAGs and independent of file order (stage 07)
- **Required first:** W06 (ordering as a planning output)
- **Used again in:** W08, C01, M04, V12
- **Outside FlowForge:** reproducible builds, stable test output
- **Test:** existing `stage_07_order_invariants`
- **Craft:** property testing (quickcheck, proptest)

#### W08 · Works at size
90 min · `now` · main step 8
- **Adds:** measure before optimising
- **Goal:** 20k-task definitions work (stage 08)
- **Required first:** W07 (determinism and invariant testing)
- **Used again in:** S01, A01, O01, TR03, TR06, Z01, Z02, G01, K01, V08, P01, AR01, AR02, AG01, AG03, AG07, AD03
- **Outside FlowForge:** any code that meets real data sizes
- **Test:** existing `stage_08_scale`
- **Craft:** allocation and profiling (oxc, inferno, memchr) · **Cookbook:** related `perf/bench_harness`, `perf/counting_allocator`, `perf/data_layout`, `memory/arena_ids`

#### W09 · When a task fails
60 min · `next` · main step 24
- **Adds:** task outcome states
- **Goal:** a run ends in a stated status; a failed task has a defined effect on everything downstream, while unrelated tasks follow a stated policy
- **Required first:** C03 (bounded concurrency)
- **Used again in:** W10, W13, W14, W17, B01, J03, R16, AB04
- **Outside FlowForge:** CI pipelines, order processing
- **Test:** random DAGs with scripted failures: no descendant of a failure starts; every task ends in exactly one terminal state; exit code matches the run status
- **Craft:** enums for lifecycles (tokio) · **Cookbook:** related `patterns/state_machine`, `patterns/generic_state_machine`

#### W10 · Try again
60 min · `next` · main step 26
- **Adds:** retry budgets
- **Goal:** failed attempts are retried within a per-task budget, and every attempt is visible
- **Required first:** W09 (task outcome states), C04 (time and choice as inputs (deterministic testing))
- **Used again in:** W11, W12, Q08, Q09
- **Outside FlowForge:** HTTP clients, job runners
- **Test:** attempts never exceed the budget; no dependent starts before a successful attempt
- **Craft:** policy separate from mechanism (tower) · **Cookbook:** similar `distributed/retry_backoff_jitter`

#### W11 · Wait before trying again
60 min · `next` · main step 27
- **Adds:** backoff with jitter
- **Goal:** retries wait with growing, randomised delays
- **Required first:** W10 (retry budgets)
- **Used again in:** Q04, T04, N03, L09
- **Outside FlowForge:** reconnect logic, cloud SDKs
- **Test:** under virtual time every delay is inside documented bounds over many seeds; other work proceeds meanwhile
- **Craft:** injected randomness (rand traits) · **Cookbook:** similar `distributed/retry_backoff_jitter`

#### W12 · Timeouts
60 min · `next` · main step 28
- **Adds:** deadlines
- **Goal:** an attempt that runs too long is ended and treated by the retry rules
- **Required first:** W10 (retry budgets) · **Helpful:** C04 (time and choice as inputs (deterministic testing))
- **Used again in:** B02, B05
- **Outside FlowForge:** every remote call
- **Test:** virtual time: durations just under and just over the limit; no completion is reported for a timed-out attempt
- **Craft:** error taxonomy, retryable vs final (anyhow, thiserror) · **Cookbook:** related `async/select_cancel`

#### W13 · Cancel a workflow
60 min · `next` · main step 33
- **Adds:** cancellation as a workflow state
- **Goal:** a cancel request moves the run and all its unfinished tasks to defined states
- **Required first:** W09 (task outcome states), C06 (cooperative cancellation)
- **Used again in:** W19, A06
- **Outside FlowForge:** checkout and booking cancellation
- **Test:** cancel after the k-th trace event, for every k: nothing starts afterwards; states are consistent; finished tasks stay finished
- **Craft:** state transitions as types (tokio-util) · **Cookbook:** related `concurrency/structured_concurrency_cancellation`

#### W14 · Conditions
90 min · `next` · branch
- **Adds:** conditional edges
- **Goal:** a task runs or is skipped depending on upstream outcomes; skipping has defined downstream effects
- **Required first:** W09 (task outcome states)
- **Used again in:** W15, W21, G02
- **Outside FlowForge:** CI "if" rules, business workflows
- **Test:** random outcomes: a task never runs when its condition is false; every task is run, skipped or blocked, exactly one of these
- **Craft:** exhaustive matching (rust-analyzer) · **Cookbook:** related `patterns/state_machine`

#### W15 · Fan-in policies
60 min · `next` · branch
- **Adds:** join policies
- **Goal:** a join can wait for all, any, or k of its inputs, with a stated rule for the rest
- **Required first:** W14 (conditional edges)
- **Used again in:** W16
- **Outside FlowForge:** approvals, parallel lookups with fallbacks
- **Test:** for random k and outcome scripts, the join starts exactly when the policy allows and never twice
- **Craft:** trait design for policies (tower)

#### W16 · Fan-out
90 min · `next` · branch
- **Adds:** map-style expansion
- **Goal:** one template task expands over a list into many instances whose results join downstream
- **Required first:** W15 (join policies)
- **Used again in:** W20, AG10
- **Outside FlowForge:** batch jobs, ETL partitions
- **Test:** instance count equals list length (including zero); the join sees every instance once; the limit from C03 still holds
- **Craft:** ownership of generated data, `Cow` (std) · **Cookbook:** related `language/cow`

#### W17 · Resume and rerun
90 min · `next` · branch
- **Adds:** workflow resumption
- **Goal:** `resume` continues a failed or interrupted run, or reruns from a chosen task, without redoing successful work
- **Required first:** D03 (recovery from durable state), W09 (task outcome states)
- **Used again in:** no planned part yet
- **Outside FlowForge:** resumable uploads, migration tools
- **Test:** crash or fail anywhere, resume: tasks recorded successful never run again; rerun-from resets exactly the affected set
- **Craft:** invariants at API boundaries (Rust API Guidelines) · **Cookbook:** related `capstones/event_sourced_ledger`

#### W18 · Versions
90 min · `next` · branch
- **Adds:** definition versioning
- **Goal:** updating a definition creates a new version; running and past runs keep the one they started with
- **Required first:** D03 (recovery from durable state), A02 (long-running operations as resources)
- **Used again in:** A10
- **Outside FlowForge:** API and schema versioning
- **Test:** update while a run is paused at a trace point: it finishes on the old version; new runs use the new one
- **Craft:** schema evolution (serde)

#### W19 · Compensation
90 min · `far` · branch
- **Adds:** sagas
- **Goal:** when a run fails, declared undo actions run for completed steps and survive restarts
- **Required first:** W13 (cancellation as a workflow state), D03 (recovery from durable state)
- **Used again in:** N21
- **Outside FlowForge:** travel booking, onboarding flows
- **Test:** every completed step with an undo action is compensated exactly once in a documented order, under crash points
- **Craft:** typestate (embedded-hal, by name) · **Cookbook:** similar `distributed/saga_orchestrator`

#### W20 · Dynamic workflows
120 min · `far` · branch
- **Adds:** dynamic DAGs
- **Goal:** a running task can add tasks; bad additions are rejected without damaging the run
- **Required first:** W16 (map-style expansion), J03 (event sourcing)
- **Used again in:** W21
- **Outside FlowForge:** agent and data pipelines
- **Test:** random expansions: the acyclic invariant always holds; replay yields the same graph
- **Craft:** API design for mutation under invariants (petgraph)

#### W21 · Control flow
120 min · `far` · branch
- **Adds:** control-flow primitives
- **Goal:** bounded loops and sub-workflows behave as first-class steps
- **Required first:** W20 (dynamic DAGs) · **Helpful:** W14 (conditional edges)
- **Used again in:** no planned part yet
- **Outside FlowForge:** workflow engines, game scripting
- **Test:** iteration bounds hold; nested runs report consistent parent and child states
- **Craft:** recursive types and arenas (oxc) · **Cookbook:** related `memory/arena_ids`

#### W capstone · Release pipeline that survives anything
`far` · 3 milestones, 4.5 h · main-path parts needed run up to step 33. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| W-C1 Run a video release workflow end to end | 90 min | a sample pipeline with conditions, fan-out, fan-in, retries and timeouts runs to a stated outcome | invariants over scripted failures under virtual time |
| W-C2 Interrupt, cancel and resume it | 90 min | kill or cancel at any event, then resume without repeating successful work | fault-point and cancel sweep |
| W-C3 Change it while it runs | 90 min | a new version and runtime-added tasks coexist with the running one | old run keeps its version; replay yields the same graph |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| definitions and validation | W01, W04, W05 |
| planning and determinism at size | W06, W07, W08 |
| failure outcomes, retries, delays, timeouts | W09, W10, W11, W12 |
| cancellation | W13 |
| conditions, joins, fan-out | W14, W15, W16 |
| resume, versions, undo | W17, W18, W19 |
| growing and looping workflows | W20, W21 |
| carried in by those parts' Required chains | A01, A02, C01, C02, C03, C04, C06, D01, D02, D03, J03 |
| stages 01-08 | assumed done |

### C · Concurrency (core)

Doing many things at once and proving it is done correctly: async, channels, locks, threads, cancellation safety, graceful shutdown, deterministic testing.

#### C01 · Sequential run
90 min · `now` · main step 9
- **Adds:** async execution
- **Goal:** `run` executes tasks one at a time with fake work and prints an event trace; this trace becomes the main test surface (ladder: sequential executor)
- **Required first:** W07 (determinism and invariant testing) · **Helpful:** W06 (ordering as a planning output)
- **Used again in:** C02, O04, TR01, Y01, AG02
- **Outside FlowForge:** any async service
- **Test:** random DAGs: each task starts and finishes once; no start before all dependency finishes; one task in flight; assertions use event order only
- **Craft:** what a future is, runtime basics (tokio) · **Cookbook:** related `async/futures_by_hand`

#### C02 · Run independent tasks together
60 min · `now` · main step 11
- **Adds:** structured concurrency
- **Goal:** `run --concurrent` runs independent tasks at the same time
- **Required first:** C01 (async execution)
- **Used again in:** C03
- **Outside FlowForge:** parallel fetches, build steps
- **Test:** replay the trace: dependencies hold; independent tasks overlap (forced with a hold-open task kind, not sleeps); every task is accounted for once
- **Craft:** spawned task ownership, `Send` and `'static` (tokio) · **Cookbook:** similar `async/worker_pool`

#### C03 · Never more than N
60 min · `now` · main step 12
- **Adds:** bounded concurrency
- **Goal:** `run --limit N` never has more than N tasks in flight (ladder: concurrent executor)
- **Required first:** C02 (structured concurrency)
- **Used again in:** W09, C04, C05, C09, S02, Q01, D01, P01
- **Outside FlowForge:** crawlers, API clients, connection pools
- **Test:** replay the trace: in flight never exceeds N, for random DAGs and N from 1 upward; with enough independent held-open work, N are in flight
- **Craft:** limits as owned resources (tokio sync) · **Cookbook:** similar `concurrency/joinset_semaphore`

#### C04 · Virtual time and seeded runs
90 min · `next` · main step 25
- **Adds:** time and choice as inputs (deterministic testing)
- **Goal:** `run --virtual-time --seed S` runs instantly and reproducibly, with timestamps in the trace
- **Required first:** C03 (bounded concurrency)
- **Used again in:** W10, W12, C06, C11, S03, S07, Q04, Q07, N01, R02, L01, L09
- **Outside FlowForge:** testing any timer-driven code
- **Test:** same seed gives byte-identical trace; long declared durations finish at once with exact virtual timestamps; earlier invariants hold over many seeds
- **Craft:** injection through traits, object safety (tokio test-util, turmoil) · **Cookbook:** related `patterns/sans_io_protocol`, `dev_tools/testing_patterns`

#### C05 · One writer, many producers
60 min · `next` · main step 29
- **Adds:** message passing to a single owner
- **Goal:** `--events <file>` streams progress live; a slow reader delays but never loses or reorders a task's events
- **Required first:** C03 (bounded concurrency)
- **Used again in:** C07, C10, Q02, A07, Y02, T01
- **Outside FlowForge:** logging pipelines, UI updates
- **Test:** read through a deliberately slow pipe: every line parses; per-task order holds; contents equal the stdout trace
- **Craft:** channels versus shared state (tokio sync) · **Cookbook:** related `async/bounded_channel`, `concurrency/shared_state`

#### C06 · Stop what is running
90 min · `next` · main step 30
- **Adds:** cooperative cancellation
- **Goal:** a stop request ends running tasks cooperatively and the trace says what happened to each
- **Required first:** C04 (time and choice as inputs (deterministic testing))
- **Used again in:** W13, C07
- **Outside FlowForge:** request abort, UI cancel buttons
- **Test:** request stop after the k-th event for every k: nothing starts afterwards; every started task has a closing event
- **Craft:** `select!` and drop (tokio-util) · **Cookbook:** related `async/select_cancel`, `concurrency/structured_concurrency_cancellation`

#### C07 · Leave cleanly
90 min · `next` · main step 31
- **Adds:** graceful shutdown
- **Goal:** on an interrupt: no new work, a grace period, then cancellation; output is flushed and the exit status is documented (ladder: cancellation)
- **Required first:** C06 (cooperative cancellation), C05 (message passing to a single owner)
- **Used again in:** C08, X03, T02
- **Outside FlowForge:** every server and worker
- **Test:** send the signal only after a named trace line; under virtual time the grace boundary is exact
- **Craft:** shutdown ordering (tokio signal) · **Cookbook:** related `async/graceful_shutdown`

#### C08 · Cancellation safety
90 min · `next` · main step 32
- **Adds:** cancel safety
- **Goal:** cancelling at any point loses and duplicates no event or result
- **Required first:** C07 (graceful shutdown)
- **Used again in:** C11, M03
- **Outside FlowForge:** any loop that waits on several things
- **Test:** a test mode cancels at each numbered suspension point in turn; the accounting invariant holds for all
- **Craft:** async cancellation safety (tokio) · **Cookbook:** related `async/async_cancel_safety`

#### C09 · CPU-bound tasks
90 min · `next` · branch
- **Adds:** threads versus async
- **Goal:** a compute task kind does not stall waiting-type tasks
- **Required first:** C03 (bounded concurrency)
- **Used again in:** no planned part yet
- **Outside FlowForge:** image, crypto and parsing work in servers
- **Test:** while compute tasks are held open, short tasks still complete (barrier style, no durations)
- **Craft:** `Send` and `Sync`, blocking pools (tokio, rayon) · **Cookbook:** related `concurrency/threads_channels`, `concurrency/thread_pool_work_stealing`

#### C10 · Look while it runs
90 min · `next` · branch
- **Adds:** shared state
- **Goal:** status requests during a live run return a consistent snapshot
- **Required first:** C05 (message passing to a single owner)
- **Used again in:** C11, O05, P03
- **Outside FlowForge:** dashboards, health pages
- **Test:** each snapshot equals the state implied by some prefix of the trace; repeated requests never deadlock
- **Craft:** locks versus snapshots, interior mutability (left-right) · **Cookbook:** related `concurrency/locks_deadlocks`, `concurrency/shared_state`

#### C11 · Explore the interleavings
120 min · `next` · branch
- **Adds:** systematic concurrency testing
- **Goal:** a test mode explores many schedules and prints a reproducing id on failure
- **Required first:** C08 (cancel safety), C10 (shared state) · **Helpful:** C04 (time and choice as inputs (deterministic testing))
- **Used again in:** S12, P05
- **Outside FlowForge:** any lock or channel protocol
- **Test:** every explored schedule satisfies earlier invariants; a planted fault is found and replays from its id
- **Craft:** model checking (loom, shuttle) · **Cookbook:** related `concurrency/model_checker_for_concurrent_code`, `distributed/deterministic_simulation_testing`

#### C capstone · Proven executor
`next` · 3 milestones, 5 h · main-path parts needed run up to step 32. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| C-C1 A run under stress | 90 min | limit, compute tasks, live events and status requests all at once with every invariant holding | all earlier trace checks together |
| C-C2 Stop it anywhere | 90 min | stop, interrupt and injected cancel at every point | accounting invariant over the sweep |
| C-C3 Show the evidence | 120 min | an exploration mode runs many schedules and writes a reproducible report | planted fault found and replayed |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| running one then many, bounded | C01, C02, C03 |
| virtual time and seeds | C04 |
| live events and snapshots | C05, C10 |
| stopping, shutdown, cancel safety | C06, C07, C08 |
| compute work | C09 |
| schedule exploration | C11 |
| an executable invariant checker | TR01 |
| stages 01-08 | assumed done |

### S · Scheduling (core)

Which eligible work runs next, where and when: ready sets, priorities, critical path, resource limits, fairness, delayed and recurring work, packing, stealing.

#### S01 · Critical path
90 min · `now` · main step 13
- **Adds:** critical-path analysis
- **Goal:** `critical-path` reports the minimum completion time with unlimited workers and one chain that causes it
- **Required first:** W08 (measure before optimising)
- **Used again in:** S04, Y04, AD02
- **Outside FlowForge:** project planning, build optimisation
- **Test:** compare with a brute-force oracle on small random DAGs; the reported chain is real and its durations sum to the total; stated tie rule gives stable output
- **Craft:** API shape for analysis results, lifetimes (petgraph)

#### S02 · Why is it waiting
60 min · `next` · main step 35
- **Adds:** block reasons
- **Goal:** `explain` says for any point of a run why each unstarted task has not started
- **Required first:** C03 (bounded concurrency)
- **Used again in:** S03, S07, Y05
- **Outside FlowForge:** "why is my pod pending" style tooling
- **Test:** replay the trace: each reason is true at that point; every unstarted task has exactly one primary reason
- **Craft:** data-carrying enums and `Display` (std)

#### S03 · Priorities
60 min · `next` · main step 36
- **Adds:** scheduling policy
- **Goal:** tasks may carry priorities; `--policy` chooses among eligible work
- **Required first:** S02 (block reasons), C04 (time and choice as inputs (deterministic testing))
- **Used again in:** S04, S05, Q03, AY03
- **Outside FlowForge:** print, ticket and job queues
- **Test:** virtual time: whenever a slot is filled, no eligible task ranks above the chosen one; dependencies and limit still hold
- **Craft:** trait design: static vs dynamic dispatch (std Ord) · **Cookbook:** related `data_structures/heap_tiebreak`, `data_structures/priority_queue_zoo`

#### S04 · Policies compared
90 min · `next` · branch
- **Adds:** heuristic scheduling
- **Goal:** a critical-path-aware policy and `compare` report the completion time of each policy
- **Required first:** S01 (critical-path analysis), S03 (scheduling policy)
- **Used again in:** AD07, MX01
- **Outside FlowForge:** evaluating any heuristic
- **Test:** no policy beats the two lower bounds (critical path, total work divided by N); equal at N=1; specific constructed graphs separate policies
- **Craft:** benchmark reports and seeded experiments (criterion) · **Cookbook:** related `os_and_storage/scheduler_policies`, `algorithms/greedy_and_interval_algorithms`

#### S05 · Resource limits
90 min · `next` · main step 38
- **Adds:** multi-resource admission
- **Goal:** tasks declare needs from named pools; runs never oversubscribe and impossible needs are rejected up front
- **Required first:** S03 (scheduling policy)
- **Used again in:** S06, S09, B05, L07, AR06, AB01
- **Outside FlowForge:** connection pools, GPU quotas
- **Test:** trace replay: usage never exceeds capacity; feasible workflows always finish (no deadlock)
- **Craft:** RAII guards for held resources (tokio sync) · **Cookbook:** related `language/drop_raii`

#### S06 · Fairness between runs
120 min · `next` · branch
- **Adds:** fair scheduling
- **Goal:** several workflows share one worker budget without any being starved
- **Required first:** S05 (multi-resource admission)
- **Used again in:** S10, L08
- **Outside FlowForge:** multi-tenant services
- **Test:** with all runs backlogged, each one's share stays within a documented bound over any window of dispatches
- **Craft:** measuring fairness, test fixtures (governor, tokio coop) · **Cookbook:** related `os_and_storage/scheduler_policies`

#### S07 · Delayed starts
60 min · `next` · main step 40
- **Adds:** time-based eligibility
- **Goal:** a task or run can be set to start no earlier than a given time
- **Required first:** C04 (time and choice as inputs (deterministic testing)) · **Helpful:** S02 (block reasons)
- **Used again in:** S08, Q04, AI02
- **Outside FlowForge:** reminders, embargoed releases
- **Test:** virtual time: nothing starts early; with free capacity, it starts at the stated instant
- **Craft:** `Instant` vs wall-clock types (jiff) · **Cookbook:** related `os_and_storage/timing_wheel`, `os_and_storage/civil_dates_time`

#### S08 · Recurring schedules
90 min · `next` · branch
- **Adds:** recurring schedules
- **Goal:** cron-like schedules create runs, with stated overlap and missed-tick rules that hold across restarts
- **Required first:** S07 (time-based eligibility), D03 (recovery from durable state)
- **Used again in:** no planned part yet
- **Outside FlowForge:** cron services, billing cycles
- **Test:** advance virtual time across month ends and downtime: expected run instants only, none duplicated; time-zone rule is stated and tested
- **Craft:** calendar types and DST (jiff) · **Cookbook:** related `os_and_storage/civil_dates_time`

#### S09 · Placement and packing
120 min · `far` · branch
- **Adds:** bin-packing heuristics
- **Goal:** tasks are placed on workers of different sizes
- **Required first:** S05 (multi-resource admission), Q11 (competing consumers)
- **Used again in:** S10, S11, S12
- **Outside FlowForge:** container placement
- **Test:** no worker oversubscribed; utilisation compared with an exact answer on tiny instances
- **Craft:** differential testing against an oracle (quickcheck) · **Cookbook:** similar `algorithms/heuristics_zoo`

#### S10 · Dominant resource fairness
120 min · `far` · branch
- **Adds:** multi-resource fairness
- **Goal:** tenants with different resource shapes get fair shares
- **Required first:** S06 (fair scheduling), S09 (bin-packing heuristics)
- **Used again in:** no planned part yet
- **Outside FlowForge:** cluster managers
- **Test:** documented fairness properties asserted over random tenant mixes
- **Craft:** numeric types and overflow (std)

#### S11 · Optimal assignment
120 min · `far` · branch
- **Adds:** optimal assignment
- **Goal:** a batch of ready tasks is assigned to workers minimising a stated cost
- **Required first:** S09 (bin-packing heuristics)
- **Used again in:** no planned part yet
- **Outside FlowForge:** dispatch and assignment systems
- **Test:** cost equals brute force on small inputs; never worse than the greedy mode
- **Craft:** generic algorithms over cost types (petgraph) · **Cookbook:** related `algorithms/linear_programming_simplex`; similar `algorithms/bipartite_matching`, `algorithms/max_flow_dinic`

#### S12 · Work stealing
120 min · `far` · branch
- **Adds:** work stealing
- **Goal:** idle workers take queued work from busy ones without breaking guarantees
- **Required first:** S09 (bin-packing heuristics), C11 (systematic concurrency testing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** runtimes and thread pools
- **Test:** each task executes under one owner; a skewed workload leaves no worker idle while others have backlog (checked on virtual-time trace)
- **Craft:** lock-free hand-off (crossbeam-deque, tokio) · **Cookbook:** similar `concurrency/thread_pool_work_stealing`

#### S capstone · Multi-tenant scheduler
`far` · 3 milestones, 5.5 h · main-path parts needed run up to step 56. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| S-C1 Shared capacity | 120 min | several tenants with priorities, resource pools and delayed and recurring work | trace never oversubscribes or starves |
| S-C2 Explain and compare | 90 min | every wait is explained and policies are compared on seeded workloads | reasons true at every point; lower bounds respected |
| S-C3 Many workers | 120 min | placement across unequal workers with idle workers taking over backlog | one owner per task; capacity respected |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| critical path | S01 |
| why a task waits, priorities, policy comparison | S02, S03, S04 |
| limits and fairness | S05, S06, S10 |
| delayed and recurring | S07, S08 |
| placement, assignment, stealing | S09, S11, S12 |
| controlled time | C04 |
| carried in by those parts' Required chains | C01, C02, C03, C05, C06, C07, C08, C10, C11, D01, D02, D03, Q01, Q02, Q04, Q05, Q06, Q07, Q09, Q11, T01, W09, W10 |
| stages 01-08 | assumed done |

### Q · Queues (core)

The path from "eligible" to "done": in memory, bounded, priority, delayed, durable, at-least-once, leases, idempotency, dead letters, admission.

#### Q01 · The ready queue made visible
45 min · `next` · main step 44
- **Adds:** the queue as a contract
- **Goal:** the trace and a summary show enqueue, dequeue and depth, with a stated ordering promise
- **Required first:** C03 (bounded concurrency)
- **Used again in:** Q02
- **Outside FlowForge:** any producer and consumer pair
- **Test:** replay: every dequeue has an earlier enqueue; depth is never negative and ends at zero; stated order holds
- **Craft:** collection choice and API surface (std) · **Cookbook:** related `std_tour/std_vec_vecdeque_methods`

#### Q02 · Bounded with backpressure
90 min · `next` · main step 45
- **Adds:** backpressure
- **Goal:** `--queue-capacity K` bounds queued work; wide graphs wait instead of growing without limit
- **Required first:** Q01 (the queue as a contract), C05 (message passing to a single owner)
- **Used again in:** Q03, Q04, Q10, P05
- **Outside FlowForge:** streaming pipelines, servers under load
- **Test:** depth never exceeds K for 20k independent tasks; every workflow still finishes, including K=1
- **Craft:** bounded channels and deadlock avoidance (tokio mpsc) · **Cookbook:** related `async/bounded_channel`, `concurrency/mpmc_bounded_queue`

#### Q03 · Priority without starvation
60 min · `next` · branch
- **Adds:** aging
- **Goal:** queued work is served by priority with a guarantee that old low-priority work eventually runs
- **Required first:** Q02 (backpressure), S03 (scheduling policy)
- **Used again in:** L08
- **Outside FlowForge:** CPU and ticket scheduling
- **Test:** under a constant high-priority arrival script, each item is served within a documented number of dispatches
- **Craft:** ordering traits (std) · **Cookbook:** related `data_structures/priority_queue_zoo`

#### Q04 · Delay queue
90 min · `next` · main step 48
- **Adds:** delayed delivery
- **Goal:** retries and delayed work wait in one place without occupying a worker slot
- **Required first:** Q02 (backpressure), C04 (time and choice as inputs (deterministic testing)) · **Helpful:** S07 (time-based eligibility), W11 (backoff with jitter)
- **Used again in:** Q05
- **Outside FlowForge:** reminder emails, retry queues
- **Test:** virtual time: never delivered early; a waiting item does not reduce available concurrency
- **Craft:** timer APIs and cancel safety (tokio-util DelayQueue) · **Cookbook:** related `os_and_storage/timing_wheel`

#### Q05 · A queue that survives
90 min · `next` · main step 49
- **Adds:** durable queue state
- **Goal:** queued work is still there after the process is killed
- **Required first:** Q04 (delayed delivery), D03 (recovery from durable state)
- **Used again in:** Q06
- **Outside FlowForge:** job libraries, mail spools
- **Test:** stop at every numbered persistence point: after restart no accepted item is missing
- **Craft:** transaction boundaries (sqlx) · **Cookbook:** related `os_and_storage/wal_append_log`, `os_and_storage/file_locking_atomic_replace`

#### Q06 · At least once
90 min · `next` · main step 50
- **Adds:** delivery guarantees
- **Goal:** work is removed only after acknowledgement; unacknowledged work is delivered again, with a delivery count
- **Required first:** Q05 (durable queue state)
- **Used again in:** Q07, X01
- **Outside FlowForge:** SQS, Pub/Sub, Kafka consumers
- **Test:** scripted lost acks: every item is eventually acknowledged; redeliveries are visible and counted
- **Craft:** must-use and consuming `self` for acks (Rust API Guidelines) · **Cookbook:** related `distributed/kafka_log_model`

#### Q07 · Leases and visibility timeouts
120 min · `next` · main step 51
- **Adds:** leases
- **Goal:** a claimed item is hidden for a lease period, can be extended, and returns if its holder goes silent
- **Required first:** Q06 (delivery guarantees), C04 (time and choice as inputs (deterministic testing))
- **Used again in:** Q08, Q09, N01, N07, N23
- **Outside FlowForge:** any claim-and-process system
- **Test:** virtual time with scripted silent workers: never two valid leases on one item; expired items return; late acks follow a stated rule
- **Craft:** time as a parameter (turmoil) · **Cookbook:** related `capstones/lease_fencing_sim`

#### Q08 · Idempotent effects
90 min · `next` · main step 52
- **Adds:** idempotency
- **Goal:** repeated delivery or repeated completion reports change state once
- **Required first:** Q07 (leases), W10 (retry budgets)
- **Used again in:** Q11, A05, X02, N04
- **Outside FlowForge:** payment APIs, event consumers
- **Test:** replay any report any number of times in any allowed order: final state and recorded effects are unchanged
- **Craft:** keys as newtypes (serde, uuid) · **Cookbook:** related `microservices/idempotency_dedup_leader_fencing_store`

#### Q09 · Dead letters
60 min · `next` · main step 54
- **Adds:** poison message handling
- **Goal:** work that keeps failing is parked with its history; `dlq list` and `dlq redrive` manage it
- **Required first:** Q07 (leases), W10 (retry budgets)
- **Used again in:** Q11
- **Outside FlowForge:** every production message system
- **Test:** always-failing items arrive after exactly the budget; healthy work is unaffected; redrive returns each once
- **Craft:** error chains preserved (anyhow) · **Cookbook:** related `errors/error_chain_retryable`

#### Q10 · Admission control
60 min · `next` · branch
- **Adds:** admission control
- **Goal:** when over its stated capacity, FlowForge refuses new runs with a clear, retryable error
- **Required first:** Q02 (backpressure)
- **Used again in:** Y07, L01, L09
- **Outside FlowForge:** API gateways
- **Test:** refusal exactly past the threshold; accepted runs are never dropped; capacity frees up again
- **Craft:** backpressure in a service trait (tower) · **Cookbook:** related `patterns/tower_service_layer`

#### Q11 · Separate worker processes
120 min · `next` · main step 56
- **Adds:** competing consumers
- **Goal:** `worker` processes pull from the shared queue; killing one loses no work
- **Required first:** Q09 (poison message handling), T01 (message framing) · **Helpful:** Q08 (idempotency)
- **Used again in:** S09, TR07, X05, T06, I03, N08
- **Outside FlowForge:** background job fleets
- **Test:** start several workers, kill one after a named event: all runs finish; effects are recorded once
- **Craft:** protocol types shared across processes (serde, tokio-util codec) · **Cookbook:** related `networking/tcp_framed_server`

#### Q capstone · FlowForge job queue
`next` · 3 milestones, 5 h · main-path parts needed run up to step 56. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| Q-C1 A bounded, fair, delayed queue | 90 min | priority and delayed work with a capacity limit and admission refusal | depth, delay and starvation bounds under virtual time |
| Q-C2 Lose nothing | 120 min | durable queue with acks, leases, idempotent effects and dead letters under kills | crash sweep: no loss, effects once |
| Q-C3 Worker fleet | 90 min | several worker processes drain it while some are killed | all runs finish; redeliveries visible |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| visible and bounded queue | Q01, Q02 |
| priority and delay | Q03, Q04 |
| durability and delivery | Q05, Q06, Q07 |
| idempotency, dead letters, admission | Q08, Q09, Q10 |
| separate workers and the wire | Q11, T01 |
| transactions and recovery | D02, D03 |
| fault points | TR04 |
| carried in by those parts' Required chains | C01, C02, C03, C04, C05, D01, S02, S03, W09, W10 |
| stages 01-08 | assumed done |

### D · Durable state (platform)

FlowForge remembers: history and restart recovery.

#### D01 · Run history
90 min · `now` · main step 14
- **Adds:** durable run records
- **Goal:** `run --state <store>` records runs; `execution get` shows them after exit
- **Required first:** C03 (bounded concurrency)
- **Used again in:** D02, A01, TR04, Y04, I01, I02, Z04, J01, AI03, AY04, AB04
- **Outside FlowForge:** any app with history or audit
- **Test:** stored states equal those implied by the trace, over random DAGs
- **Craft:** database errors and migrations (sqlx or rusqlite) · **Cookbook:** related `microservices/transactional_outbox_inbox_sqlite`

#### D02 · All of a change, or none
60 min · `now` · main step 15
- **Adds:** transactions
- **Goal:** a state change that touches several records is never visible half done
- **Required first:** D01 (durable run records)
- **Used again in:** D03
- **Outside FlowForge:** bank transfers, order placement
- **Test:** stop between the writes at each fault flag: readers see the old state or the new one
- **Craft:** transaction guards and rollback on drop (sqlx) · **Cookbook:** related `microservices/transactional_outbox_inbox_sqlite`

#### D03 · Restart recovery
120 min · `now` · main step 16
- **Adds:** recovery from durable state
- **Goal:** after a kill, unfinished runs are found and completed (ladder: persistence/restart)
- **Required first:** D02 (transactions)
- **Used again in:** W17, W18, W19, S08, Q05, A02, X01, M04, J03, N11, K02, E01, H01, V16, AD05
- **Outside FlowForge:** payment and import jobs
- **Test:** a fault flag stops the process after the n-th committed change, for every n: recovery completes; committed successes never rerun; the policy for in-flight tasks is stated
- **Craft:** transactions and type-level state (sqlx, sled) · **Cookbook:** related `os_and_storage/wal_append_log`, `patterns/generic_state_machine`

#### D capstone · Kill it anywhere
`next` · 2 milestones, 2.5 h · main-path parts needed run up to step 53. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| D-C1 Crash campaign over a run | 90 min | stop at every committed change during a mixed workflow and recover | no committed success reruns; no half changes |
| D-C2 Recovery report | 60 min | `recover --explain` states what was found and done | report matches the injected fault |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| records and transactions | D01, D02 |
| recovery | D03 |
| named fault points | TR04 |
| the run trace | C01 |
| lifecycle outcomes | W09 |
| carried in by those parts' Required chains | C02, C03 |
| stages 01-08 | assumed done |

### A · Networking and web: API and CLI (platform)

The public surface: REST, live events, security of access, and a client that uses only the API.

#### A01 · Definitions over HTTP
90 min · `now` · main step 17
- **Adds:** HTTP resources with one error shape
- **Goal:** `serve` accepts, stores and returns definitions with one error shape (ladder: REST API, part 1)
- **Required first:** W08 (measure before optimising), D01 (durable run records)
- **Used again in:** A02, Z03
- **Outside FlowForge:** any REST service
- **Test:** port 0, server prints its address; the same random good and bad documents get the same verdict as `validate`; every error has the documented shape and status
- **Craft:** extractors and error responses (axum) · **Cookbook:** related `web/axum_errors`, `web/rest_api_patterns`

#### A02 · Runs over HTTP
90 min · `now` · main step 18
- **Adds:** long-running operations as resources
- **Goal:** start a run and poll run and task status (ladder: REST API, part 2)
- **Required first:** A01 (HTTP resources with one error shape), D03 (recovery from durable state)
- **Used again in:** W18, A03, A04, A05, A07, Y01, X03, M01, L01, AI05
- **Outside FlowForge:** export and render job APIs
- **Test:** status only moves forward; after server restart, status is preserved and runs finish
- **Craft:** shared application state (axum) · **Cookbook:** related `capstones/job_service_axum`

#### A03 · CLI client
90 min · `now` · main step 19
- **Adds:** a client bound only to the public contract
- **Goal:** `workflow create/list`, `run`, `execution get` work against a server (ladder: CLI client)
- **Required first:** A02 (long-running operations as resources)
- **Used again in:** A06, A09, TR05
- **Outside FlowForge:** developer tools for any API
- **Test:** CLI output agrees with direct HTTP reads; unreachable server and API errors give exit 1 with clean stderr
- **Craft:** derive-based CLI design (clap, cargo) · **Cookbook:** related `dev_tools/clap_cli`

#### A04 · Pages that stay stable
60 min · `next` · branch
- **Adds:** cursor pagination
- **Goal:** list endpoints paginate, filter and sort stably while data changes
- **Required first:** A02 (long-running operations as resources)
- **Used again in:** A11, Z05
- **Outside FlowForge:** feeds and admin lists
- **Test:** insert between pages: no item that existed throughout is skipped or repeated
- **Craft:** opaque cursor types (axum, serde) · **Cookbook:** related `web/rest_api_patterns`

#### A05 · Safe retries for clients
60 min · `next` · branch
- **Adds:** request idempotency
- **Goal:** an idempotency key makes repeated "start run" create one run
- **Required first:** A02 (long-running operations as resources), Q08 (idempotency)
- **Used again in:** no planned part yet
- **Outside FlowForge:** payment and order APIs
- **Test:** concurrent and post-restart repeats yield one run; same key with a different body is refused
- **Craft:** middleware vs handler (tower) · **Cookbook:** similar `web/axum_idempotency`

#### A06 · Cancel through the API
45 min · `next` · branch
- **Adds:** remote cancellation
- **Goal:** cancel from HTTP and CLI
- **Required first:** A03 (a client bound only to the public contract), W13 (cancellation as a workflow state)
- **Used again in:** no planned part yet
- **Outside FlowForge:** long-running job control
- **Test:** cancel after observing a state: eventually terminal; repeated cancel is harmless
- **Craft:** channel choice for control signals (tokio watch) · **Cookbook:** related `async/select_cancel`

#### A07 · Live events
60 min · `next` · branch
- **Adds:** server-sent event streaming
- **Goal:** an SSE stream shows a run's events live
- **Required first:** A02 (long-running operations as resources), C05 (message passing to a single owner)
- **Used again in:** A08
- **Outside FlowForge:** progress bars, notifications, token streaming
- **Test:** the streamed events equal the trace; a slow client does not stall the engine
- **Craft:** `Stream` and pinning (axum, tokio-stream) · **Cookbook:** similar `web/axum_sse_replay`

#### A08 · Resume a stream
60 min · `next` · branch
- **Adds:** resumable streams (ladder: SSE)
- **Goal:** a client reconnects from the last event it saw
- **Required first:** A07 (server-sent event streaming)
- **Used again in:** A09, R17
- **Outside FlowForge:** change feeds, chat, sync
- **Test:** disconnect after any event and reconnect: the joined stream equals an uninterrupted one
- **Craft:** cursor types (axum) · **Cookbook:** similar `web/axum_sse_replay`

#### A09 · Watch from the CLI
60 min · `next` · branch
- **Adds:** reconnecting client
- **Goal:** `execution watch` follows a run and survives a server restart
- **Required first:** A03 (a client bound only to the public contract), A08 (resumable streams (ladder: SSE))
- **Used again in:** no planned part yet
- **Outside FlowForge:** log tailing tools
- **Test:** restart mid-stream: printed events have no gaps or repeats
- **Craft:** client retry design (reqwest) · **Cookbook:** related `web/http_client_resilience`

#### A10 · Concurrent updates
60 min · `next` · branch
- **Adds:** optimistic concurrency
- **Goal:** two editors cannot silently overwrite each other
- **Required first:** W18 (definition versioning)
- **Used again in:** no planned part yet
- **Outside FlowForge:** document and settings editing
- **Test:** of simultaneous updates from one base version exactly one succeeds
- **Craft:** version newtypes (axum, headers) · **Cookbook:** related `web/http_caching_conditional`

#### A11 · A described API
45 min · `next` · branch
- **Adds:** a machine-readable contract
- **Goal:** an OpenAPI document that matches the running server
- **Required first:** A04 (cursor pagination)
- **Used again in:** no planned part yet
- **Outside FlowForge:** client generation and API reviews
- **Test:** every documented route and error shape is exercised; undocumented routes fail the check
- **Craft:** schema derivation (utoipa)

#### A capstone · The public product
`next` · 3 milestones, 4 h · main-path parts needed run up to step 52. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| A-C1 Full workflow through the CLI | 90 min | create, run, watch, cancel and inspect against a server | end-to-end CLI suite equals direct HTTP reads |
| A-C2 Hostile network client | 90 min | retried creates, disconnects and concurrent edits | one run per key; gapless stream; one winning edit |
| A-C3 Documented and guarded | 60 min | the OpenAPI document and access rules cover every route | route and permission matrix |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| resources and runs over HTTP | A01, A02 |
| CLI client and watch | A03, A07, A08, A09 |
| paging, safe retries, cancel, concurrent edits | A04, A05, A06, A10 |
| API description | A11 |
| who may call and what they may do | Z05, Z06 |
| workflow cancel and versions | W13, W18 |
| carried in by those parts' Required chains | C01, C02, C03, C04, C05, C06, D01, D02, D03, Q01, Q02, Q04, Q05, Q06, Q07, Q08, W09, W10 |
| stages 01-08 | assumed done |

### O · Ownership and API design (subject)

The engine as a library that is pleasant and safe to use.

#### O01 · The engine is a library
60 min · `next` · main step 21
- **Adds:** a public crate boundary
- **Goal:** the engine is usable without the CLI; CLI behaviour unchanged
- **Required first:** W08 (measure before optimising)
- **Used again in:** O02, O03, O06
- **Outside FlowForge:** every reusable Rust project
- **Test:** CLI suites unchanged; documented examples compile as doctests
- **Craft:** visibility, `lib` plus `bin` (ripgrep) · **Cookbook:** related `language/modules_visibility`

#### O02 · Errors callers can act on
60 min · `next` · main step 22
- **Adds:** stable error contracts
- **Goal:** every failure has a stable code, shown by `--error-format json`
- **Required first:** O01 (a public crate boundary)
- **Used again in:** O04, O07, TR05, T06
- **Outside FlowForge:** SDKs, APIs
- **Test:** each failure class yields its documented code; human text may change without breaking tests
- **Craft:** library versus application errors (thiserror, anyhow) · **Cookbook:** related `errors/error_chain_retryable`

#### O03 · Hard to misuse
90 min · `next` · branch
- **Adds:** compile-time API states
- **Goal:** building workflows in code cannot skip required steps
- **Required first:** O01 (a public crate boundary)
- **Used again in:** O07
- **Outside FlowForge:** client libraries, config builders
- **Test:** `compile_fail` doctests prove misuse does not build
- **Craft:** builders and typestate (std process Command) · **Cookbook:** related `patterns/typestate_builder`, `patterns/typestate_protocol`

#### O04 · Add a task kind
90 min · `next` · main step 59
- **Adds:** extension through traits
- **Goal:** new task kinds plug in without editing the scheduler; `kinds` lists them
- **Required first:** O02 (stable error contracts), C01 (async execution)
- **Used again in:** O05, T04, B01
- **Outside FlowForge:** plugins, drivers, middleware
- **Test:** a second kind passes the same trace invariants; unknown kinds give a located error
- **Craft:** object safety, generics versus `dyn` (tower) · **Cookbook:** related `language/dyn_vs_generic`, `patterns/type_erasure_vtable`

#### O05 · One engine, many owners
90 min · `next` · branch
- **Adds:** ownership across tasks (handles)
- **Goal:** one engine serves several concurrent runs through a small handle
- **Required first:** O04 (extension through traits), C10 (shared state)
- **Used again in:** no planned part yet
- **Outside FlowForge:** any long-lived service object
- **Test:** concurrent runs keep per-run invariants; shutdown leaves nothing running
- **Craft:** handles, `Arc`, channels (tokio, left-right) · **Cookbook:** related `language/smart_pointers`, `concurrency/shared_state`

#### O06 · Borrow, do not copy
90 min · `far` · branch
- **Adds:** lifetimes in APIs
- **Goal:** large definitions load with fewer allocations, reported by a stats flag
- **Required first:** O01 (a public crate boundary), P01 (repeatable measurement)
- **Used again in:** no planned part yet
- **Outside FlowForge:** parsers, protocol decoders
- **Test:** identical behaviour; allocation count under a budget
- **Craft:** borrowed parsing and `Cow` (serde) · **Cookbook:** related `language/lifetimes`, `language/cow`

#### O07 · Keep the promise
60 min · `far` · branch
- **Adds:** API evolution
- **Goal:** the public API is documented, feature-gated and checked for accidental breaks
- **Required first:** O02 (stable error contracts), O03 (compile-time API states)
- **Used again in:** no planned part yet
- **Outside FlowForge:** publishing any crate
- **Test:** doctests, feature matrix and a semver check pass
- **Craft:** Rust API Guidelines (cargo-semver-checks) · **Cookbook:** related `language/library_api_polish`

#### O capstone · flowforge as a crate
`far` · 2 milestones, 2.5 h · main-path parts needed run up to step 59. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| O-C1 Embed it | 90 min | a separate example program builds and runs workflows through the public API only | it compiles against the public surface and its output equals the CLI's |
| O-C2 Publish-ready | 60 min | docs, features, stable error codes and a compatibility check | doctests, feature matrix, semver check |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| library boundary and errors | O01, O02 |
| misuse-proof building | O03 |
| extension and shared handle | O04, O05 |
| borrowing API | O06 |
| API evolution | O07 |
| running tasks | C01 |
| carried in by those parts' Required chains | C02, C03, C05, C10, P01 |
| stages 01-08 | assumed done |

### TR · Testing and reliability (subject)

Tools that show FlowForge is right and keep it right. The mentor still writes the stage tests; these parts give the product its testing affordances.

#### TR01 · Check my trace
45 min · `next` · main step 10
- **Adds:** invariants as an executable oracle
- **Goal:** `check-trace <file>` judges any recorded run against the run invariants
- **Required first:** C01 (async execution)
- **Used again in:** TR02
- **Outside FlowForge:** validating logs from any system
- **Test:** hand-broken traces are rejected for the right reason; real traces pass
- **Craft:** tests beside code (std) · **Cookbook:** related `dev_tools/testing_patterns`

#### TR02 · Make your own inputs
60 min · `next` · branch
- **Adds:** input generators
- **Goal:** `generate --seed` produces valid and deliberately invalid workflows of chosen shapes
- **Required first:** TR01 (invariants as an executable oracle)
- **Used again in:** TR03
- **Outside FlowForge:** property testing, load data
- **Test:** same seed, same output; every valid one validates and every invalid one fails for its labelled reason
- **Craft:** generator traits (quickcheck Arbitrary) · **Cookbook:** related `math_science/prng_zoo`, `dev_tools/testing_patterns`

#### TR03 · Shrink the failure
90 min · `next` · branch
- **Adds:** test case shrinking
- **Goal:** `minimise` reduces a failing definition to a small one that fails the same way
- **Required first:** TR02 (input generators), W08 (measure before optimising)
- **Used again in:** Y06
- **Outside FlowForge:** bug reports in any codebase
- **Test:** result fails identically; removing any one more element makes it pass
- **Craft:** property testing (proptest, quickcheck) · **Cookbook:** similar `dev_tools/shrinking`

#### TR04 · Fail on purpose
90 min · `next` · main step 53
- **Adds:** fault injection points
- **Goal:** named fault points can be listed and switched on
- **Required first:** D01 (durable run records)
- **Used again in:** TR07, I02
- **Outside FlowForge:** testing recovery in any service
- **Test:** every listed point is reachable; each injected fault gives a documented outcome
- **Craft:** test hooks behind features (fail-rs, by name) · **Cookbook:** related `distributed/deterministic_simulation_testing`

#### TR05 · The API contract
60 min · `next` · branch
- **Adds:** contract testing
- **Goal:** recorded request and response contracts guard the public surface
- **Required first:** A03 (a client bound only to the public contract), O02 (stable error contracts)
- **Used again in:** no planned part yet
- **Outside FlowForge:** service-to-service compatibility
- **Test:** incompatible changes fail; additive ones pass
- **Craft:** snapshot testing (insta)

#### TR06 · Fuzz what parses
60 min · `far` · branch
- **Adds:** coverage-guided fuzzing
- **Goal:** fuzz targets for definitions and log records with a saved corpus
- **Required first:** W08 (measure before optimising), K02 (write-ahead logging)
- **Used again in:** no planned part yet
- **Outside FlowForge:** every parser of untrusted input
- **Test:** the corpus replays without crashes in normal tests
- **Craft:** harness design (regex) · **Cookbook:** related `text_and_parsing/binary_format_parsing`

#### TR07 · A local chaos campaign
120 min · `far` · branch
- **Adds:** reliability campaigns
- **Goal:** a seeded campaign kills and restarts processes and writes a report
- **Required first:** TR04 (fault injection points), Q11 (competing consumers)
- **Used again in:** N23
- **Outside FlowForge:** game days, soak tests
- **Test:** each campaign ends with all invariants checked; failures replay from the seed
- **Craft:** test orchestration (nextest)

#### TR capstone · Reliability workbench
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 56. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| TR-C1 Find, shrink, replay | 90 min | generate inputs, find a violation of a planted fault, shrink it and replay it | the planted fault is found and reduced |
| TR-C2 Seeded campaign report | 120 min | a campaign over processes and fault points with one report | every failure replays from its seed |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| trace checking | TR01 |
| input generation and shrinking | TR02, TR03 |
| fault points | TR04 |
| API contracts | TR05 |
| fuzzing | TR06 |
| process campaigns | TR07 |
| carried in by those parts' Required chains | A01, A02, A03, C01, C02, C03, C04, C05, D01, D02, D03, K01, K02, O01, O02, Q01, Q02, Q04, Q05, Q06, Q07, Q09, Q11, T01, W09, W10 |
| stages 01-08 | assumed done |

### Y · Monitoring and debugging (subject)

Seeing what a running or finished FlowForge did.

#### Y01 · Logs you can query
60 min · `next` · main step 46
- **Adds:** structured logging context
- **Goal:** structured logs carry run, task and request ids across boundaries
- **Required first:** A02 (long-running operations as resources) · **Helpful:** C01 (async execution)
- **Used again in:** Y02, Y03, Z04
- **Outside FlowForge:** any web service or batch job
- **Test:** every line caused by a request carries its id; all output parses
- **Craft:** events and fields (tracing) · **Cookbook:** related `dev_tools/tracing_logs`

#### Y02 · Numbers that add up
90 min · `next` · main step 47
- **Adds:** metrics
- **Goal:** a metrics report and endpoint cover depth, latency and errors
- **Required first:** Y01 (structured logging context), C05 (message passing to a single owner)
- **Used again in:** Y05, Y07, X06
- **Outside FlowForge:** any production service
- **Test:** counters reconcile exactly with the trace once idle; no latency thresholds in tests
- **Craft:** atomics and histograms (metrics-rs) · **Cookbook:** related `dev_tools/metrics_prometheus_histograms_load_testing`

#### Y03 · One request, one trace
60 min · `next` · branch
- **Adds:** distributed tracing
- **Goal:** spans link a request to every task attempt it caused
- **Required first:** Y01 (structured logging context)
- **Used again in:** Y06
- **Outside FlowForge:** microservice debugging
- **Test:** the exported span tree matches the event trace
- **Craft:** spans across await points (tracing) · **Cookbook:** related `dev_tools/tracing_logs`

#### Y04 · Where did the time go?
90 min · `next` · branch
- **Adds:** post-run time attribution
- **Goal:** `timeline <run>` separates running, waiting on dependencies and waiting for capacity
- **Required first:** S01 (critical-path analysis), D01 (durable run records)
- **Used again in:** no planned part yet
- **Outside FlowForge:** CI pipeline and query profiling
- **Test:** under virtual time the parts sum exactly to each task's elapsed time
- **Craft:** report types (cargo timings)

#### Y05 · It looks stuck
60 min · `next` · branch
- **Adds:** stall detection
- **Goal:** a watchdog flags runs with no progress and says why
- **Required first:** Y02 (metrics), S02 (block reasons)
- **Used again in:** no planned part yet
- **Outside FlowForge:** job queues, ETL monitoring
- **Test:** virtual time: planted stalls are flagged, healthy long tasks are not
- **Craft:** health as data (tokio-console)

#### Y06 · A bundle that reproduces
60 min · `next` · branch
- **Adds:** reproducible diagnostics
- **Goal:** one command exports what is needed to replay a bad run, with secrets removed
- **Required first:** Y03 (distributed tracing), TR03 (test case shrinking)
- **Used again in:** no planned part yet
- **Outside FlowForge:** support tooling
- **Test:** replaying the bundle gives the same trace; planted secrets are absent
- **Craft:** redacting wrapper types (secrecy)

#### Y07 · Load experiments
90 min · `far` · branch
- **Adds:** measurement under load
- **Goal:** a load command drives arrival-rate scenarios and reports saturation
- **Required first:** Y02 (metrics), Q10 (admission control)
- **Used again in:** no planned part yet
- **Outside FlowForge:** capacity planning
- **Test:** work accounting reconciles; reports reproducible in the simulator; no thresholds
- **Craft:** latency recording (hdrhistogram) · **Cookbook:** related `dev_tools/metrics_prometheus_histograms_load_testing`

#### Y capstone · Debug a production incident
`far` · 3 milestones, 3.5 h · main-path parts needed run up to step 47. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| Y-C1 Instrument a broken run | 60 min | logs, metrics and request traces describe a run with planted problems | the three views reconcile |
| Y-C2 Diagnose from the bundle | 90 min | stall detection, time attribution and a bundle identify each planted cause | bundle replays; causes reported |
| Y-C3 Under load | 60 min | a load scenario shows where saturation begins | accounting reconciles |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| logs, metrics, traces | Y01, Y02, Y03 |
| time attribution and stalls | Y04, Y05 |
| reproducible bundle | Y06 |
| load | Y07 |
| why a task waits | S02 |
| carried in by those parts' Required chains | A01, A02, C01, C02, C03, C05, D01, D02, D03, Q01, Q02, Q10, S01, TR01, TR02, TR03 |
| stages 01-08 | assumed done |

### X · Delivery and operations (platform)

Events out, deployable service.

#### X01 · Transactional outbox
90 min · `next` · main step 57
- **Adds:** outbox
- **Goal:** state changes and their outgoing events are never lost relative to each other
- **Required first:** D03 (recovery from durable state), Q06 (delivery guarantees)
- **Used again in:** X02, T05
- **Outside FlowForge:** order and billing services
- **Test:** crash at every point: every committed change is eventually published; consumers may see repeats, never gaps
- **Craft:** transaction-scoped APIs (sqlx) · **Cookbook:** similar `microservices/transactional_outbox_inbox_sqlite`

#### X02 · Idempotent consumers
60 min · `next` · main step 58
- **Adds:** consumer deduplication
- **Goal:** an audit consumer stays correct under duplicate and reordered delivery
- **Required first:** X01 (outbox), Q08 (idempotency)
- **Used again in:** X05
- **Outside FlowForge:** projections, notifications
- **Test:** shuffled and duplicated feeds produce the same audit view
- **Craft:** trait boundaries for brokers (async-trait or native) · **Cookbook:** related `distributed/kafka_log_model`

#### X03 · Ready, alive, draining
60 min · `next` · branch
- **Adds:** operability
- **Goal:** health and readiness endpoints, signal-driven drain, an image and CI (mentor explains setup directly)
- **Required first:** A02 (long-running operations as resources), C07 (graceful shutdown)
- **Used again in:** X04
- **Outside FlowForge:** any deployed service
- **Test:** after an observed event the signal drains cleanly and readiness flips first
- **Craft:** CI and lints (jonhoo/rust-ci-conf) · **Cookbook:** related `async/graceful_shutdown`

#### X04 · One config, three sources
45 min · `next` · branch
- **Adds:** layered configuration
- **Goal:** file, environment and flags combine by a documented precedence, and secrets are never printed
- **Required first:** X03 (operability)
- **Used again in:** X06
- **Outside FlowForge:** any deployed service
- **Test:** a precedence table is asserted; unknown keys are rejected
- **Craft:** typed config (serde, figment by name)

#### X05 · Real broker
90 min · `next` · branch
- **Adds:** adapting to at-least-once infrastructure
- **Goal:** the same consumer behaviour over Pub/Sub, locally against an emulator
- **Required first:** X02 (consumer deduplication), Q11 (competing consumers)
- **Used again in:** no planned part yet
- **Outside FlowForge:** moving to a managed broker in any system
- **Test:** the X02 suite passes against both transports; opt-in, outside default tests
- **Craft:** feature flags (tokio)

#### X06 · Deploy
120 min · `next` · branch
- **Adds:** infrastructure as code
- **Goal:** Cloud Run and database via Terraform (explained directly)
- **Required first:** X04 (layered configuration), Y02 (metrics)
- **Used again in:** no planned part yet
- **Outside FlowForge:** every cloud deployment
- **Test:** `terraform validate` and an opt-in smoke run of the A03 suite against the URL
- **Craft:** workspace layout (cargo)

#### X capstone · FlowForge in the cloud
`next` · 2 milestones, 3.5 h · main-path parts needed run up to step 58. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| X-C1 Events leave safely | 90 min | outbox, consumer and managed broker deliver every change | consumer suite passes on both transports |
| X-C2 Deploy and roll | 120 min | a deployed service drains cleanly across a redeploy | opt-in smoke suite; local drain test (mentor explains the cloud setup directly) |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| outbox and consumers | X01, X02 |
| operability and configuration | X03, X04 |
| managed broker | X05 |
| deployment | X06 |
| metrics and logs | Y01, Y02 |
| crash recovery | D03 |
| carried in by those parts' Required chains | A01, A02, C01, C02, C03, C04, C05, C06, C07, D01, D02, Q01, Q02, Q04, Q05, Q06, Q07, Q08, Q09, Q11, T01, W09, W10 |
| stages 01-08 | assumed done |

### T · Networking: wire and protocols (subject)

Sockets, framing, outgoing calls, webhooks, RPC.

#### T01 · A wire protocol
90 min · `next` · main step 55
- **Adds:** message framing
- **Goal:** two FlowForge processes exchange framed messages over TCP
- **Required first:** C05 (message passing to a single owner)
- **Used again in:** Q11, T02, T03
- **Outside FlowForge:** any TCP protocol
- **Test:** the test splits and joins bytes at every boundary; oversized or garbage frames close the connection cleanly
- **Craft:** codecs (tokio-util) · **Cookbook:** related `networking/tcp_framed_server`

#### T02 · Slow and rude peers
60 min · `next` · branch
- **Adds:** connection lifecycle
- **Goal:** idle, slow and half-closed connections are handled with limits
- **Required first:** T01 (message framing), C07 (graceful shutdown)
- **Used again in:** I04, N08
- **Outside FlowForge:** every server
- **Test:** scripted socket misbehaviour never blocks other clients; assertions use events
- **Craft:** cancellation on drop (tokio) · **Cookbook:** related `std_tour/std_net_tcp_udp_ip_addr`

#### T03 · HTTP by hand, once
90 min · `far` · branch
- **Adds:** HTTP/1.1 on the wire
- **Goal:** a minimal health endpoint without a web framework, as a study part
- **Required first:** T01 (message framing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** debugging proxies and clients
- **Test:** a table of raw requests including malformed ones
- **Craft:** incremental parser API (httparse) · **Cookbook:** similar `networking/http1_parser`

#### T04 · An HTTP task kind
90 min · `next` · branch
- **Adds:** safe retrying clients
- **Goal:** a task calls an HTTP endpoint with retries that respect method safety
- **Required first:** O04 (extension through traits), W11 (backoff with jitter)
- **Used again in:** T05, Z07
- **Outside FlowForge:** calling third-party APIs
- **Test:** against a scripted flaky local server, unsafe requests are never repeated without a key
- **Craft:** client reuse and builders (reqwest) · **Cookbook:** related `web/http_client_resilience`

#### T05 · Webhooks
90 min · `far` · branch
- **Adds:** signed webhook delivery
- **Goal:** run events are delivered to subscribers, signed and retried
- **Required first:** T04 (safe retrying clients), X01 (outbox)
- **Used again in:** no planned part yet
- **Outside FlowForge:** integrations of every kind
- **Test:** the test receiver verifies signatures, rejects replays and sees at-least-once delivery
- **Craft:** constant-time comparison (subtle) · **Cookbook:** related `web/webhooks_signatures`

#### T06 · An internal gRPC contract
90 min · `far` · branch
- **Adds:** schema-first RPC
- **Goal:** the worker protocol is also offered over gRPC
- **Required first:** Q11 (competing consumers), O02 (stable error contracts)
- **Used again in:** no planned part yet
- **Outside FlowForge:** internal service APIs
- **Test:** the worker suite passes over both; an older client still works
- **Craft:** generated code boundaries (tonic, prost) · **Cookbook:** related `networking/protobuf_grpc_wire_format`

#### T capstone · Integrations gateway
`far` · 2 milestones, 3 h · main-path parts needed run up to step 59. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| T-C1 Talk to workers | 90 min | the wire protocol and gRPC serve the same worker suite | both transports pass |
| T-C2 Talk to the world | 90 min | HTTP tasks and signed webhooks against hostile local servers | no unsafe repeat; signatures verified |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| framing and peers | T01, T02 |
| HTTP on the wire | T03 |
| outgoing calls and webhooks | T04, T05 |
| RPC contract | T06 |
| outbox | X01 |
| retry delays | W11 |
| carried in by those parts' Required chains | C01, C02, C03, C04, C05, C06, C07, D01, D02, D03, O01, O02, O04, Q01, Q02, Q04, Q05, Q06, Q07, Q09, Q11, W09, W10 |
| stages 01-08 | assumed done |

### B · Task sandboxing (subject)

Running real, untrusted task code.

#### B01 · Run a real command
90 min · `next` · main step 60
- **Adds:** child processes
- **Goal:** a command task kind captures exit status and output
- **Required first:** O04 (extension through traits), W09 (task outcome states)
- **Used again in:** B02, B05
- **Outside FlowForge:** CI runners, build tools
- **Test:** exit codes, large output on both streams and missing programs give documented outcomes without hanging
- **Craft:** `Command` and pipe handling (tokio process, duct) · **Cookbook:** related `std_tour/std_process_command_pipelines`

#### B02 · Stop the whole family
90 min · `next` · branch
- **Adds:** process groups
- **Goal:** timeout or cancel ends every process a task started
- **Required first:** B01 (child processes), W12 (deadlines)
- **Used again in:** B03, B04, I03
- **Outside FlowForge:** test runners, job supervisors
- **Test:** a task that spawns children leaves none alive, checked through pids
- **Craft:** unix extensions and cfg (nix) · **Cookbook:** related `os_and_storage/process_pipes_signals`

#### B03 · Bounded output and time
60 min · `far` · branch
- **Adds:** resource limits
- **Goal:** output size and CPU or memory limits are enforced where the platform supports it
- **Required first:** B02 (process groups)
- **Used again in:** no planned part yet
- **Outside FlowForge:** online judges, CI
- **Test:** hostile programs end with the matching error; unsupported platforms say so
- **Craft:** platform cfg (rlimit, by name) · **Cookbook:** related `os_and_storage/syscalls_portable`

#### B04 · A private room
90 min · `far` · branch
- **Adds:** per-attempt isolation
- **Goal:** each attempt gets its own directory and an explicit environment
- **Required first:** B02 (process groups), Z03 (path safety)
- **Used again in:** no planned part yet
- **Outside FlowForge:** hermetic builds
- **Test:** a probe task sees only declared variables and files; cleanup on every exit path
- **Craft:** cleanup on drop (tempfile) · **Cookbook:** related `language/drop_raii`

#### B05 · WASM tasks
120 min · `far` · branch
- **Adds:** sandboxed execution
- **Goal:** tasks can be sandboxed WASM modules with fuel and memory limits
- **Required first:** B01 (child processes), S05 (multi-resource admission) · **Helpful:** W12 (deadlines)
- **Used again in:** B06
- **Outside FlowForge:** plugin systems, edge functions
- **Test:** hostile modules are stopped with the right error; the engine survives
- **Craft:** trust boundaries (wasmtime) · **Cookbook:** related `compilers_runtimes/wasm_binary_interpreter`

#### B06 · Only what you grant
90 min · `far` · branch
- **Adds:** capability-based host interfaces
- **Goal:** host functions are granted per task
- **Required first:** B05 (sandboxed execution), Z07 (SSRF defence)
- **Used again in:** no planned part yet
- **Outside FlowForge:** extensions and mini-apps
- **Test:** ungranted calls fail; granted ones are audited
- **Craft:** capability types (wasmtime-wasi) · **Cookbook:** related `compilers_runtimes/wasm_binary_interpreter`

#### B capstone · Run untrusted tasks
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 60. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| B-C1 Command tasks, contained | 90 min | command tasks with limits, private directories and full cleanup | hostile probe programs are contained as documented |
| B-C2 Plugin tasks | 120 min | WASM tasks with only granted host functions and an outgoing policy | ungranted and over-budget actions fail |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| processes and trees | B01, B02 |
| limits and isolation | B03, B04 |
| WASM and grants | B05, B06 |
| paths and outgoing policy | Z03, Z07 |
| timeouts and resource limits | W12, S05 |
| carried in by those parts' Required chains | A01, C01, C02, C03, C04, D01, O01, O02, O04, S02, S03, T04, W09, W10, W11 |
| stages 01-08 | assumed done |

### I · Operating systems (subject)

Processes, files, locks, supervision, kernel I/O.

#### I01 · Only one of me
45 min · `next` · branch
- **Adds:** file locking
- **Goal:** a second process on the same store is refused, and a dead holder does not block forever
- **Required first:** D01 (durable run records)
- **Used again in:** no planned part yet
- **Outside FlowForge:** cron jobs, package managers
- **Test:** barrier-style two-process test; kill the holder and start again
- **Craft:** guard types for OS locks (fs4, by name) · **Cookbook:** related `os_and_storage/file_locking_atomic_replace`

#### I02 · What a flush promises
60 min · `far` · branch
- **Adds:** filesystem durability
- **Goal:** durability levels are selectable and documented
- **Required first:** D01 (durable run records), TR04 (fault injection points)
- **Used again in:** I05
- **Outside FlowForge:** config files, databases
- **Test:** fault points between steps: the stated old-or-new guarantee holds in a simulated file layer
- **Craft:** io traits for testability (std io) · **Cookbook:** related `os_and_storage/file_disk_io_patterns`, `os_and_storage/file_locking_atomic_replace`

#### I03 · Supervise the workers
90 min · `far` · branch
- **Adds:** process supervision
- **Goal:** `supervise` keeps a worker pool alive under a restart policy
- **Required first:** B02 (process groups), Q11 (competing consumers)
- **Used again in:** no planned part yet
- **Outside FlowForge:** init systems, orchestrators
- **Test:** killed workers return; a crash loop stops after the documented limit
- **Craft:** signal handling (tokio signal) · **Cookbook:** related `os_and_storage/process_pipes_signals`

#### I04 · Many sockets, one thread
120 min · `far` · branch
- **Adds:** OS event notification
- **Goal:** an optional transport built on readiness events
- **Required first:** T02 (connection lifecycle)
- **Used again in:** no planned part yet
- **Outside FlowForge:** how async runtimes work
- **Test:** the T01 and T02 suites pass on both transports
- **Craft:** safe wrappers over syscalls (mio) · **Cookbook:** related `networking/mio_event_loop`, `networking/kqueue_epoll_raw`

#### I05 · A completion-based log writer
120 min · `far` · branch
- **Adds:** completion-based I/O
- **Goal:** a Linux-only I/O backend
- **Required first:** I02 (filesystem durability), K02 (write-ahead logging)
- **Used again in:** no planned part yet
- **Outside FlowForge:** high-throughput servers
- **Test:** K02 crash suite passes on both backends; skipped cleanly elsewhere
- **Craft:** buffer ownership in async I/O (tokio-uring) · **Cookbook:** related `os_and_storage/io_uring_model`

#### I capstone · A well-behaved daemon
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 60. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| I-C1 Supervised local deployment | 90 min | single-instance server and supervised workers survive kills and restarts | no double owner; crash loops stop |
| I-C2 Durable and portable I/O | 120 min | durability modes and alternative I/O backends pass the same suites | fault and parity suites |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| single instance | I01 |
| durability | I02 |
| supervision | I03 |
| readiness and completion I/O | I04, I05 |
| child processes | B02 |
| worker processes | Q11 |
| carried in by those parts' Required chains | B01, C01, C02, C03, C04, C05, C06, C07, D01, D02, D03, K01, K02, O01, O02, O04, Q01, Q02, Q04, Q05, Q06, Q07, Q09, T01, T02, TR04, W09, W10, W12 |
| stages 01-08 | assumed done |

### M · Caches and memory (subject)

Reusing work and spending memory on purpose.

#### M01 · Do not prepare twice
45 min · `next` · main step 61
- **Adds:** caching a derived value
- **Goal:** repeated use of one definition reuses prepared work, visible in stats
- **Required first:** A02 (long-running operations as resources)
- **Used again in:** M02
- **Outside FlowForge:** web request handling
- **Test:** results equal uncached; hit and miss counts exact for a script
- **Craft:** `Arc` sharing (moka)

#### M02 · A cache with a budget
90 min · `next` · branch
- **Adds:** eviction policy
- **Goal:** the cache never exceeds its limit and stats show evictions
- **Required first:** M01 (caching a derived value)
- **Used again in:** M03
- **Outside FlowForge:** CDNs, database buffers
- **Test:** capacity invariant on random scripts; matches the documented policy against a reference model
- **Craft:** safe linked structures (lru, moka) · **Cookbook:** related `data_structures/cache_policy_zoo`; similar `data_structures/lru_cache`

#### M03 · One miss, one computation
60 min · `next` · branch
- **Adds:** request coalescing
- **Goal:** concurrent misses for a key compute once, even if the first caller is cancelled
- **Required first:** M02 (eviction policy), C08 (cancel safety)
- **Used again in:** no planned part yet
- **Outside FlowForge:** cache stampede protection
- **Test:** barrier-style: one computation, all callers get result or error
- **Craft:** cancel-safe shared futures (moka, tokio OnceCell) · **Cookbook:** related `language/static_const_lazy`

#### M04 · Skip unchanged tasks
120 min · `next` · branch
- **Adds:** result caching
- **Goal:** a task whose definition and inputs are unchanged reuses its result, and `--explain-cache` says why
- **Required first:** D03 (recovery from durable state), W07 (determinism and invariant testing)
- **Used again in:** MX02
- **Outside FlowForge:** Bazel, cargo, CI caches
- **Test:** change one thing at a time: exactly the tasks that could differ rerun
- **Craft:** hashes stable across runs (blake3) · **Cookbook:** related `data_structures/hash_functions`

#### M05 · Where the bytes are
90 min · `far` · branch
- **Adds:** memory accounting
- **Goal:** memory per task and run is reported and kept under a budget
- **Required first:** P01 (repeatable measurement)
- **Used again in:** M06
- **Outside FlowForge:** any memory-bound service
- **Test:** budget holds on fixed workloads; behaviour unchanged
- **Craft:** layout and allocation counting (oxc) · **Cookbook:** related `perf/data_layout`, `perf/counting_allocator`, `perf/struct_packing`

#### M06 · Allocate in one piece
90 min · `far` · branch
- **Adds:** arena allocation
- **Goal:** an optional arena mode for per-run data
- **Required first:** M05 (memory accounting)
- **Used again in:** no planned part yet
- **Outside FlowForge:** compilers, game frames, request scratch
- **Test:** suites pass under it; usage accounting balances
- **Craft:** arena lifetimes (bumpalo, oxc_allocator) · **Cookbook:** related `memory/bump_arena_allocator`, `memory/slab_generational`

#### M capstone · The incremental engine
`far` · 2 milestones, 3 h · main-path parts needed run up to step 61. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| M-C1 Rebuild only what changed | 90 min | repeat runs reuse results, with bounded caches and no duplicate computation | rerun set equals what could differ |
| M-C2 Within a memory budget | 90 min | large workloads stay under stated memory limits | counters under budget, behaviour unchanged |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| caching and eviction | M01, M02 |
| one computation per miss | M03 |
| result reuse and invalidation | M04 |
| memory accounting and arenas | M05, M06 |
| determinism | W07 |
| cancel safety | C08 |
| carried in by those parts' Required chains | A01, A02, C01, C02, C03, C04, C05, C06, C07, D01, D02, D03, P01 |
| stages 01-08 | assumed done |

### Z · Security (subject)

Hostile input, secrets, outgoing access, audit.

#### Z01 · What could go wrong
45 min · `next` · branch
- **Adds:** threat modelling
- **Goal:** a written list of FlowForge's assets, entry points and trust boundaries, where each risk points to a check or is openly accepted (a document you write)
- **Required first:** W08 (measure before optimising)
- **Used again in:** no planned part yet
- **Outside FlowForge:** design review of any system
- **Test:** the mentor reviews it; every listed mitigation names an existing or planned black-box check
- **Craft:** security notes beside code (rustls docs, by name) · **Cookbook:** related `web/security_pitfalls_secure_coding`

#### Z02 · Hostile definitions
60 min · `next` · branch
- **Adds:** limits on untrusted input
- **Goal:** size, depth and count limits reject abusive input clearly
- **Required first:** W08 (measure before optimising)
- **Used again in:** no planned part yet
- **Outside FlowForge:** uploads, public APIs
- **Test:** oversized and deeply nested documents fail cleanly within a work budget
- **Craft:** parser limits (serde_json recursion limit) · **Cookbook:** related `web/security_pitfalls_secure_coding`

#### Z03 · Paths stay inside
60 min · `next` · branch
- **Adds:** path safety
- **Goal:** ids and artifact names can never escape the state directory
- **Required first:** A01 (HTTP resources with one error shape)
- **Used again in:** B04, Z07
- **Outside FlowForge:** file servers, archive extraction
- **Test:** generated hostile names never create files outside a temp root
- **Craft:** `Path` and `OsStr` (cap-std) · **Cookbook:** related `web/security_pitfalls_secure_coding`

#### Z04 · Secrets stay secret
60 min · `next` · branch
- **Adds:** secret handling
- **Goal:** tasks use named secrets that never appear in logs, traces, errors or stored state
- **Required first:** D01 (durable run records), Y01 (structured logging context)
- **Used again in:** no planned part yet
- **Outside FlowForge:** any service holding credentials
- **Test:** planted canary values are absent from every output surface
- **Craft:** redacting types (secrecy, zeroize)

#### Z05 · Who is calling
60 min · `next` · branch
- **Adds:** authentication
- **Goal:** the API accepts only authenticated callers
- **Required first:** A04 (cursor pagination)
- **Used again in:** Z06
- **Outside FlowForge:** every network API
- **Test:** a matrix of missing, malformed, expired and valid credentials; no endpoint is unprotected by omission
- **Craft:** extractors as capability types (axum) · **Cookbook:** related `web/auth_basic_bearer_jwt`

#### Z06 · What may they do
60 min · `next` · branch
- **Adds:** authorization
- **Goal:** callers see and change only what they own
- **Required first:** Z05 (authentication)
- **Used again in:** Z08, AB02
- **Outside FlowForge:** multi-tenant SaaS
- **Test:** matrix of caller, resource and verb; no cross-owner leakage in lists or streams
- **Craft:** capability types (axum) · **Cookbook:** related `web/backend_authz_rbac_abac_policies`

#### Z07 · Where may a task call?
90 min · `far` · branch
- **Adds:** SSRF defence
- **Goal:** outgoing requests obey an allow policy that redirects cannot bypass
- **Required first:** T04 (safe retrying clients), Z03 (path safety)
- **Used again in:** B06
- **Outside FlowForge:** webhooks, link previewers, scrapers
- **Test:** a local test server with redirects and odd address forms: forbidden targets are never contacted
- **Craft:** URL and IP types (url, std net) · **Cookbook:** related `web/security_pitfalls_secure_coding`

#### Z08 · An audit trail you can trust
90 min · `far` · branch
- **Adds:** tamper-evident logs
- **Goal:** privileged actions are recorded and tampering is detected
- **Required first:** Z06 (authorization), J03 (event sourcing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** audit and compliance logs
- **Test:** any edit, deletion or reorder in the file is reported
- **Craft:** hash newtypes (RustCrypto) · **Cookbook:** related `cryptography/merkle_tree`

#### Z capstone · Security review of FlowForge
`far` · 2 milestones, 2.5 h · main-path parts needed run up to step 59. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| Z-C1 Attack suite | 90 min | hostile input, paths, destinations and credentials are all refused | every threat-list entry has a passing check |
| Z-C2 Evidence | 60 min | secrets never leak and privileged actions are tamper-evident | canaries absent; edited audit detected |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| threat model | Z01 |
| input, paths, secrets | Z02, Z03, Z04 |
| identity and permissions | Z05, Z06 |
| outgoing access | Z07 |
| audit | Z08 |
| carried in by those parts' Required chains | A01, A02, A04, C01, C02, C03, C04, D01, D02, D03, J03, O01, O02, O04, T04, W09, W10, W11, Y01 |
| stages 01-08 | assumed done |

### J · Lineage and event history (subject)

Where results came from and what happened, over time.

#### J01 · What a task reads and writes
45 min · `far` · branch
- **Adds:** provenance records
- **Goal:** tasks declare input and output artifacts; each run records them
- **Required first:** D01 (durable run records), W04 (semantic validation)
- **Used again in:** J02
- **Outside FlowForge:** data pipelines, audits
- **Test:** records match trace and definition exactly
- **Craft:** owned versus borrowed records (serde)

#### J02 · Where did this come from?
90 min · `far` · branch
- **Adds:** data lineage
- **Goal:** `lineage <artifact>` shows what produced it and what depends on it, across runs
- **Required first:** J01 (provenance records), AG01 (What does this change touch?)
- **Used again in:** J05, V13
- **Outside FlowForge:** dbt and warehouse lineage, build provenance
- **Test:** matches a naive oracle over recorded runs
- **Craft:** graph APIs (petgraph)

#### J03 · History as the truth
90 min · `far` · branch
- **Adds:** event sourcing
- **Goal:** current state can be rebuilt from an append-only history; `replay` verifies it
- **Required first:** D03 (recovery from durable state), W09 (task outcome states)
- **Used again in:** W20, Z08, J04, J06
- **Outside FlowForge:** ledgers, order systems
- **Test:** rebuilt equals stored for random runs; every prefix is a legal state
- **Craft:** pure transition functions, event versioning (serde) · **Cookbook:** related `patterns/generic_state_machine`; similar `capstones/event_sourced_ledger`

#### J04 · Go back in time
90 min · `far` · branch
- **Adds:** temporal queries
- **Goal:** `execution at <event>` shows past state and diffs two points
- **Required first:** J03 (event sourcing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** debugging production incidents
- **Test:** state at k equals replay of the first k events; diffs compose
- **Craft:** iterator adapters over histories (std)

#### J05 · Ask about lineage
120 min · `far` · branch
- **Adds:** a graph query language
- **Goal:** a small query form over relationships with filters
- **Required first:** J02 (data lineage), E01 (a query surface)
- **Used again in:** no planned part yet
- **Outside FlowForge:** authorization graphs, code search
- **Test:** random queries agree with a reference evaluator
- **Craft:** parser design (winnow) · **Cookbook:** related `database/datalog_engine_semi_naive`

#### J06 · Old history
90 min · `far` · branch
- **Adds:** retention with snapshots
- **Goal:** retention and snapshots bound history without changing answers inside the retained window
- **Required first:** J03 (event sourcing), K03 (checkpointing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** any event store
- **Test:** answers equal before and after for retained points
- **Craft:** versioned snapshots (serde) · **Cookbook:** similar `capstones/event_sourced_ledger`

#### J capstone · Explain any result
`far` · 2 milestones, 3 h · main-path parts needed run up to step 24. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| J-C1 Provenance across runs | 90 min | from an artifact, show what made it, what depends on it, and query it | agrees with oracle |
| J-C2 Replay the past | 90 min | show past state and keep answers within retention | replay equals stored; retention preserves answers |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| provenance and lineage | J01, J02 |
| event history and time travel | J03, J04 |
| queries and retention | J05, J06 |
| what a change affects | AG01 |
| query surface | E01 |
| carried in by those parts' Required chains | C01, C02, C03, D01, D02, D03, K01, K02, K03, W09 |
| stages 01-08 | assumed done |

### N · Distributed systems (progression)

FlowForge across machines, first in a deterministic simulator.

#### N01 · Two processes, only messages
120 min · `far` · main step 62
- **Adds:** process boundaries in a deterministic simulator
- **Goal:** `sim` runs a coordinator and workers that interact only through messages inside one deterministic simulation
- **Required first:** C04 (time and choice as inputs (deterministic testing)), Q07 (leases)
- **Used again in:** N02
- **Outside FlowForge:** testing any protocol or client-server pair
- **Test:** same seed gives the same history; with a perfect network the run satisfies the single-process trace invariants
- **Craft:** sans-IO design (turmoil) · **Cookbook:** related `patterns/sans_io_protocol`

#### N02 · An unreliable network
90 min · `far` · main step 63
- **Adds:** an explicit fault model
- **Goal:** the simulator can drop, delay, duplicate, reorder and partition messages from a seed or script
- **Required first:** N01 (process boundaries in a deterministic simulator)
- **Used again in:** N03, N10
- **Outside FlowForge:** reviewing any design that crosses a network
- **Test:** each fault type is visible in the history and replayable; invariant violations are reported, including ones that now appear
- **Craft:** seeded fault plans (turmoil) · **Cookbook:** related `distributed/deterministic_simulation_testing`

#### N03 · No answer is not "no"
90 min · `far` · main step 64
- **Adds:** ambiguous outcomes
- **Goal:** unanswered requests end with an outcome that admits what is unknown
- **Required first:** N02 (an explicit fault model), W11 (backoff with jitter)
- **Used again in:** N04, N05
- **Outside FlowForge:** payment calls, cloud SDKs
- **Test:** no wait is unbounded; retry budget holds over many seeds; unknown outcomes are never reported as failures
- **Craft:** error types for unknown outcomes (tonic status codes, by name) · **Cookbook:** related `web/http_client_resilience`

#### N04 · Delivery guarantees end to end
90 min · `far` · main step 65
- **Adds:** end-to-end delivery reasoning
- **Goal:** each message path states its guarantee, and effects stay correct under duplicates and loss
- **Required first:** N03 (ambiguous outcomes), Q08 (idempotency)
- **Used again in:** N07, N11
- **Outside FlowForge:** queues, webhooks, ingestion pipelines
- **Test:** over fault seeds: every accepted run completes after the network heals; no effect is recorded twice
- **Craft:** request identity as a type (uuid) · **Cookbook:** related `microservices/idempotency_dedup_leader_fencing_store`

#### N05 · Is it dead?
60 min · `far` · main step 66
- **Adds:** failure detection
- **Goal:** the coordinator suspects silent workers and reports suspicion, recovery and wrong suspicions
- **Required first:** N03 (ambiguous outcomes)
- **Used again in:** N06, N07, N14
- **Outside FlowForge:** load balancer health checks
- **Test:** crashed workers are suspected within a bounded number of simulated periods; a slow one may be wrongly suspected and safety still holds
- **Craft:** timer ownership (tokio time)

#### N06 · Everyone learns who is alive
120 min · `far` · branch
- **Adds:** SWIM gossip
- **Goal:** members share failure information without a central observer
- **Required first:** N05 (failure detection)
- **Used again in:** no planned part yet
- **Outside FlowForge:** cluster membership in databases and service meshes
- **Test:** live members converge within a bounded number of rounds; message load per member is reported
- **Craft:** protocol as a pure state machine (foca) · **Cookbook:** similar `distributed/swim_gossip`

#### N07 · Fencing
90 min · `far` · main step 67
- **Adds:** fencing tokens
- **Goal:** a paused, outdated worker cannot commit after ownership moved on
- **Required first:** N04 (end-to-end delivery reasoning), N05 (failure detection) · **Helpful:** Q07 (leases)
- **Used again in:** N08, N09
- **Outside FlowForge:** distributed locks, leader-only writes
- **Test:** scripted pause past lease expiry: the stale write is refused
- **Craft:** capability types (etcd-client, by name) · **Cookbook:** similar `capstones/lease_fencing_sim`

#### N08 · Real remote workers
120 min · `far` · main step 68
- **Adds:** one protocol, two transports
- **Goal:** the protocol proven in simulation runs over real sockets
- **Required first:** N07 (fencing tokens), Q11 (competing consumers) · **Helpful:** T02 (connection lifecycle)
- **Used again in:** no planned part yet
- **Outside FlowForge:** agents, build farms
- **Test:** a test proxy drops and duplicates at named points; effects recorded once; the simulator suite still passes
- **Craft:** framing and cancellation (tokio-util) · **Cookbook:** related `networking/tcp_framed_server`

#### N09 · Partitioned scheduling
90 min · `far` · branch
- **Adds:** partitioning
- **Goal:** several schedulers each own part of the runs and rebalance
- **Required first:** N07 (fencing tokens)
- **Used again in:** N20, N22
- **Outside FlowForge:** sharded caches and databases
- **Test:** one owner per run per epoch; movement on a membership change is bounded
- **Craft:** stable hashing (std Hash caveats) · **Cookbook:** similar `distributed/consistent_hashing`, `distributed/hash_partitioning`

#### N10 · Clocks that disagree
60 min · `far` · branch
- **Adds:** ordering across physical clocks (hybrid clocks)
- **Goal:** histories merged from nodes with skewed clocks respect cause and effect
- **Required first:** N02 (an explicit fault model) · **Helpful:** R06 (logical clocks)
- **Used again in:** N12
- **Outside FlowForge:** log correlation, multi-region databases
- **Test:** no effect precedes its cause in the merged view under skew
- **Craft:** ordering types (std Ord) · **Cookbook:** related `distributed/clock_sync_and_trace_context`; similar `distributed/vector_clocks_hlc`

#### N11 · Copies of run state
90 min · `far` · branch
- **Adds:** replication
- **Goal:** run state is copied to followers; the tool shows what is lost or stale when the leader dies
- **Required first:** N04 (end-to-end delivery reasoning), D03 (recovery from durable state)
- **Used again in:** N12, N13
- **Outside FlowForge:** database replicas and failover
- **Test:** in each documented mode the loss and staleness bounds hold over fault seeds
- **Craft:** a trait for replication transport (openraft, by name)

#### N12 · How many copies must answer
90 min · `far` · branch
- **Adds:** quorums
- **Goal:** configurable read and write counts, and the tool shows which settings can return stale data
- **Required first:** N11 (replication), N10 (ordering across physical clocks (hybrid clocks))
- **Used again in:** no planned part yet
- **Outside FlowForge:** Cassandra and Dynamo style tuning
- **Test:** safe settings never miss an acknowledged write across seeds; for unsafe ones a counterexample replays
- **Craft:** validated configuration types (std) · **Cookbook:** similar `distributed/quorum_replication_dynamo`

#### N13 · Was that history legal?
90 min · `far` · branch
- **Adds:** linearizability checking
- **Goal:** `sim check` judges a recorded history against a stated consistency model
- **Required first:** N11 (replication)
- **Used again in:** N14
- **Outside FlowForge:** testing any shared store, lock or queue
- **Test:** hand-written legal and illegal histories get the right verdict; a planted replication bug is found
- **Craft:** search with explicit budgets (stateright, by name) · **Cookbook:** related `distributed/linearizability_checker`

#### N14 · Raft 1: one leader
120 min · `far` · branch
- **Adds:** consensus: elections
- **Goal:** schedulers elect at most one leader per term
- **Required first:** N13 (linearizability checking), N05 (failure detection)
- **Used again in:** N15
- **Outside FlowForge:** etcd, Consul, control planes
- **Test:** at most one leader per term across seeded partitions; a leader appears after healing
- **Craft:** state machine design (raft-rs, openraft) · **Cookbook:** similar `distributed/raft_consensus`

#### N15 · Raft 2: one log
120 min · `far` · branch
- **Adds:** consensus: log replication
- **Goal:** execution events are replicated and applied in one order
- **Required first:** N14 (consensus: elections)
- **Used again in:** N16, N20
- **Outside FlowForge:** replicated state machines
- **Test:** committed entries never change; all nodes apply a common prefix
- **Craft:** storage traits (openraft) · **Cookbook:** similar `distributed/raft_consensus`

#### N16 · Raft 3: crashes and disks
120 min · `far` · branch
- **Adds:** what consensus must persist
- **Goal:** nodes restart from disk without breaking safety
- **Required first:** N15 (consensus: log replication), K02 (write-ahead logging)
- **Used again in:** N17
- **Outside FlowForge:** any durable protocol
- **Test:** crash at every persistence point including torn writes: the safety suite holds
- **Craft:** durability in signatures (raft-rs) · **Cookbook:** similar `distributed/raft_consensus`

#### N17 · Raft 4: clients
90 min · `far` · branch
- **Adds:** client semantics over consensus
- **Goal:** commands through a changing leader take effect once and reads are not stale
- **Required first:** N16 (what consensus must persist)
- **Used again in:** N18, N23
- **Outside FlowForge:** using etcd or ZooKeeper correctly
- **Test:** the N13 checker passes on client histories with leader crashes
- **Craft:** session types (openraft) · **Cookbook:** similar `distributed/raft_consensus`

#### N18 · Raft 5: bounded logs
90 min · `far` · branch
- **Adds:** consensus snapshots
- **Goal:** logs stay bounded and a lagging node catches up
- **Required first:** N17 (client semantics over consensus), K03 (checkpointing)
- **Used again in:** N19
- **Outside FlowForge:** operating a consensus cluster
- **Test:** the safety suite holds while nodes catch up from snapshots
- **Craft:** versioned messages (openraft)

#### N19 · Raft 6: new members
90 min · `far` · branch
- **Adds:** membership reconfiguration
- **Goal:** nodes can be added and replaced while the cluster serves
- **Required first:** N18 (consensus snapshots)
- **Used again in:** no planned part yet
- **Outside FlowForge:** replacing machines in any cluster
- **Test:** the safety suite holds through every scripted change
- **Craft:** versioned messages (openraft)

#### N20 · All partitions or none
90 min · `far` · branch
- **Adds:** two-phase commit
- **Goal:** an operation spanning partitions commits atomically
- **Required first:** N09 (partitioning), N15 (consensus: log replication)
- **Used again in:** N21
- **Outside FlowForge:** cross-shard transactions
- **Test:** crash sweep: no partial commit; the blocking case is demonstrated
- **Craft:** typestate for protocol phases (Rust API Guidelines) · **Cookbook:** similar `distributed/two_phase_commit`

#### N21 · Undo instead of lock
90 min · `far` · branch
- **Adds:** sagas across nodes
- **Goal:** the same operation done with compensation, compared with N20
- **Required first:** N20 (two-phase commit), W19 (sagas)
- **Used again in:** no planned part yet
- **Outside FlowForge:** booking and order flows
- **Test:** every partial failure ends compensated; visible intermediate states are documented
- **Craft:** durable step logs (temporal sdk-core) · **Cookbook:** similar `distributed/saga_orchestrator`

#### N22 · One limit, many nodes
120 min · `far` · branch
- **Adds:** distributed rate limiting
- **Goal:** several instances enforce one limit
- **Required first:** L05 (generic cell rate algorithm), N09 (partitioning)
- **Used again in:** no planned part yet
- **Outside FlowForge:** API gateways
- **Test:** with scripted partitions admissions stay within the documented overshoot
- **Craft:** atomic scripts and client errors (redis-rs)

#### N23 · Break it on purpose
120 min · `far` · branch
- **Adds:** fault-injection campaigns
- **Goal:** a fault campaign across the cluster with a written report
- **Required first:** N17 (client semantics over consensus), TR07 (reliability campaigns) · **Helpful:** Q07 (leases)
- **Used again in:** no planned part yet
- **Outside FlowForge:** Jepsen-style review of any system
- **Test:** histories satisfy the stated model over many seeds; planted bugs are caught and replayed
- **Craft:** checker design (stateright) · **Cookbook:** related `distributed/linearizability_checker`, `distributed/deterministic_simulation_testing`

#### N capstone · Replicated FlowForge
`far` · 3 milestones, 6 h · main-path parts needed run up to step 68. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| N-C1 Cluster in the simulator | 120 min | partitioned, replicated schedulers with fenced workers under all fault types | safety invariants over many seeds |
| N-C2 Consensus-backed history | 120 min | run history survives minority loss and client retries | checker passes on client histories |
| N-C3 Real processes and a fault campaign | 120 min | the same logic on sockets with a written campaign report | planted bugs caught; report replays |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| simulator and fault model | N01, N02 |
| unknown outcomes, delivery, detection, membership | N03, N04, N05, N06 |
| fencing, partitioning, real transport | N07, N08, N09 |
| clocks, replication, quorums, history checking | N10, N11, N12, N13 |
| consensus | N14, N15, N16, N17, N18, N19 |
| cross-partition work | N20, N21 |
| cluster limits and campaigns | N22, N23 |
| queue leases | Q07 |
| trustworthy disk records | K01, K02 |
| carried in by those parts' Required chains | C01, C02, C03, C04, C05, C06, D01, D02, D03, K03, L01, L03, L05, Q01, Q02, Q04, Q05, Q06, Q08, Q09, Q10, Q11, T01, TR04, TR07, W09, W10, W11, W13, W19 |
| stages 01-08 | assumed done |

### R · CRDT: replicas and merging (progression)

FlowForge state edited or updated on disconnected replicas.

#### R01 · Two copies, two answers
45 min · `far` · branch
- **Adds:** why merging replicas is hard
- **Goal:** `merge base left right` on offline edits shows which obvious merges depend on order or lose edits
- **Required first:** W04 (semantic validation)
- **Used again in:** R02
- **Outside FlowForge:** git merges, offline mobile apps
- **Test:** scripted edit pairs: each conflict class is reported; nothing is silently dropped
- **Craft:** equality and identity (std)

#### R02 · Convergence you can test
60 min · `far` · branch
- **Adds:** convergence properties as invariants
- **Goal:** `sim replicas` delivers updates in seeded orders and reports whether copies agree
- **Required first:** R01 (why merging replicas is hard), C04 (time and choice as inputs (deterministic testing))
- **Used again in:** R03, R04, R05
- **Outside FlowForge:** reviewing any sync or reduce operation
- **Test:** reorder, regroup and duplicate deliveries; a deliberately broken merge is caught with its seed
- **Craft:** property tests of trait laws (quickcheck) · **Cookbook:** similar `distributed/crdts`

#### R03 · Counting while apart
60 min · `far` · branch
- **Adds:** replicated counters
- **Goal:** run statistics from disconnected workers combine to correct totals
- **Required first:** R02 (convergence properties as invariants)
- **Used again in:** no planned part yet
- **Outside FlowForge:** likes, quotas and metrics across regions
- **Test:** all delivery orders and duplicates give exact totals
- **Craft:** algebraic trait design (std) · **Cookbook:** similar `distributed/crdts`

#### R04 · Add and remove while apart
90 min · `far` · branch
- **Adds:** replicated sets
- **Goal:** task tags edited on two replicas converge by a stated add-versus-remove rule
- **Required first:** R02 (convergence properties as invariants)
- **Used again in:** R08
- **Outside FlowForge:** shopping carts, membership lists
- **Test:** generated concurrent histories match a reference model in the test
- **Craft:** generic bounds (std collections) · **Cookbook:** similar `distributed/crdts`

#### R05 · One value, two writers
60 min · `far` · branch
- **Adds:** replicated registers
- **Goal:** single fields converge, and the tool says what was kept and discarded
- **Required first:** R02 (convergence properties as invariants)
- **Used again in:** R06
- **Outside FlowForge:** settings and profile sync
- **Test:** the stated conflict policy holds under reordered delivery
- **Craft:** multi-value types (std) · **Cookbook:** similar `distributed/crdts`

#### R06 · Which came after which
60 min · `far` · branch
- **Adds:** logical clocks
- **Goal:** updates carry logical time so merges respect cause
- **Required first:** R05 (replicated registers)
- **Used again in:** N10, R07
- **Outside FlowForge:** event ordering in any distributed log
- **Test:** an update made after seeing another never loses to it
- **Craft:** clock newtypes (std Ord) · **Cookbook:** similar `distributed/vector_clocks_hlc`

#### R07 · Newer, or just different?
90 min · `far` · branch
- **Adds:** detecting concurrency (vector clocks)
- **Goal:** the tool distinguishes "newer" from "concurrent" and surfaces real conflicts
- **Required first:** R06 (logical clocks)
- **Used again in:** R08, R11
- **Outside FlowForge:** Dynamo-style stores, sync engines
- **Test:** verdicts match a happened-before relation built by the test
- **Craft:** a truly partial `PartialOrd` (std) · **Cookbook:** similar `distributed/vector_clocks_hlc`

#### R08 · Maps of mergeable things
90 min · `far` · branch
- **Adds:** composing replicated types
- **Goal:** the collection of tasks with mergeable fields converges, including edit versus delete
- **Required first:** R04 (replicated sets), R07 (detecting concurrency (vector clocks))
- **Used again in:** R09, R10, R12, R16
- **Outside FlowForge:** local-first apps
- **Test:** composite histories converge; the edit-versus-delete policy holds
- **Craft:** generic composition (automerge) · **Cookbook:** related `distributed/crdt_todo_app_no_db`; similar `distributed/crdts`

#### R09 · Converged but invalid
120 min · `far` · branch
- **Adds:** global invariants versus convergence
- **Goal:** two valid edits can merge into an invalid workflow; replicas agree on how it is reported or resolved
- **Required first:** R08 (composing replicated types), W05 (the DAG invariant)
- **Used again in:** R17
- **Outside FlowForge:** any collaborative editor with rules (schemas, calendars)
- **Test:** all replicas reach the same verdict and the same runnable definition
- **Craft:** validate after merge (automerge)

#### R10 · Send the change, not the state
90 min · `far` · branch
- **Adds:** operation-based replication
- **Goal:** replicas exchange operations; delivery requirements are explicit and enforced
- **Required first:** R08 (composing replicated types)
- **Used again in:** R11, R14
- **Outside FlowForge:** collaboration servers, change feeds
- **Test:** over a faulty channel the delivery layer restores the stated guarantee; results match state-based mode
- **Craft:** operation enums (yrs) · **Cookbook:** related `distributed/operational_transform`; similar `distributed/crdts`

#### R11 · Send only what is missing
90 min · `far` · branch
- **Adds:** delta-based synchronisation
- **Goal:** sync transfers what the peer lacks and reports bytes
- **Required first:** R10 (operation-based replication), R07 (detecting concurrency (vector clocks))
- **Used again in:** R13
- **Outside FlowForge:** mobile sync, anti-entropy
- **Test:** same final state as full sync; transfer bounded by divergence on constructed cases
- **Craft:** compact encodings (yrs, automerge)

#### R12 · What deletion costs
60 min · `far` · branch
- **Adds:** tombstones
- **Goal:** `doc stats` shows metadata left behind by deletions
- **Required first:** R08 (composing replicated types)
- **Used again in:** R13
- **Outside FlowForge:** any soft-delete or replicated store
- **Test:** stats exact against a scripted history; naive removal is shown to revive data
- **Craft:** measuring memory (std) · **Cookbook:** related `distributed/crdt_todo_app_no_db`

#### R13 · Forgetting safely
120 min · `far` · branch
- **Adds:** garbage collection by causal stability
- **Goal:** old metadata is reclaimed without reviving deleted items, even with a long-offline replica
- **Required first:** R12 (tombstones), R11 (delta-based synchronisation)
- **Used again in:** R17
- **Outside FlowForge:** log compaction, MVCC cleanup
- **Test:** seeded schedules with offline replicas: no revival; metadata shrinks once all catch up
- **Craft:** reclamation as an API contract (crossbeam-epoch)

#### R14 · An ordered list of steps
120 min · `far` · branch
- **Adds:** sequence CRDTs
- **Goal:** concurrent insert and delete in an ordered step list converge sensibly
- **Required first:** R10 (operation-based replication)
- **Used again in:** R15, R17
- **Outside FlowForge:** playlists, task boards
- **Test:** convergence over seeds; each replica's own ordering intent is preserved; known anomalies tested by example
- **Craft:** large sequence design (yrs, diamond-types) · **Cookbook:** similar `distributed/crdt_fugue_text_editor`

#### R15 · Typing at the same time
120 min · `far` · branch
- **Adds:** text CRDTs
- **Goal:** a task script is edited concurrently and stays converged
- **Required first:** R14 (sequence CRDTs)
- **Used again in:** no planned part yet
- **Outside FlowForge:** collaborative documents and code
- **Test:** random concurrent edit traces converge and satisfy stated intent rules
- **Craft:** `str` boundaries and graphemes (ropey) · **Cookbook:** similar `distributed/crdt_fugue_text_editor`

#### R16 · Reports from offline workers
90 min · `far` · branch
- **Adds:** designing a mergeable domain state
- **Goal:** workers that ran while disconnected report back; run state converges without contradicting terminal states
- **Required first:** R08 (composing replicated types), W09 (task outcome states)
- **Used again in:** no planned part yet
- **Outside FlowForge:** field devices, offline point of sale
- **Test:** any arrival order and duplication gives one state; illegal transitions never appear
- **Craft:** lifecycle types (automerge) · **Cookbook:** related `patterns/state_machine`

#### R17 · Edit one workflow together
120 min · `far` · branch
- **Adds:** none new: composition
- **Goal:** clients edit one definition through the server and converge after offline periods (composes earlier parts)
- **Required first:** R09 (global invariants versus convergence), R14 (sequence CRDTs) · **Helpful:** R13 (garbage collection by causal stability), A08 (resumable streams (ladder: SSE))
- **Used again in:** no planned part yet
- **Outside FlowForge:** building any multiplayer editor
- **Test:** scripted clients with partitions: identical documents and verdicts afterwards
- **Craft:** API boundaries around a CRDT core (yrs) · **Cookbook:** related `distributed/crdt_block_editor_notion_like`

#### R capstone · Offline-first workflow editor
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 29. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| R-C1 Merge definitions and run state | 90 min | offline replicas of definitions and run reports converge to valid state | seeded partition schedules |
| R-C2 Live shared editing | 120 min | clients edit steps and script text together through the server, with deletion cleanup | identical documents and verdicts; metadata bounded |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| why merging is hard and invariants | R01, R02 |
| counters, sets, registers | R03, R04, R05 |
| causality | R06, R07 |
| maps and validity | R08, R09 |
| operations, deltas | R10, R11 |
| deletion and cleanup | R12, R13 |
| sequences and text | R14, R15 |
| run state and composition | R16, R17 |
| live events to clients | A08 |
| carried in by those parts' Required chains | A01, A02, A07, C01, C02, C03, C04, C05, D01, D02, D03, W09 |
| stages 01-08 | assumed done |

### L · Rate limiting and overload (subject)

Protection of the API and worker admission. Cross-node limiting lives in N.

#### L01 · Windowed counting
45 min · `far` · branch
- **Adds:** window-based limiting
- **Goal:** per-tenant request limits with a retry hint
- **Required first:** Q10 (admission control), C04 (time and choice as inputs (deterministic testing)) · **Helpful:** A02 (long-running operations as resources)
- **Used again in:** L02, L03
- **Outside FlowForge:** API quotas
- **Test:** scripted arrivals: decisions match a naive reference in the test; boundary behaviour is documented
- **Craft:** keyed state and eviction (governor) · **Cookbook:** similar `capstones/rate_limiter_lab`

#### L02 · Sliding windows
60 min · `far` · branch
- **Adds:** log vs approximate counter
- **Goal:** smoother limits in two variants
- **Required first:** L01 (window-based limiting)
- **Used again in:** L06
- **Outside FlowForge:** API quotas
- **Test:** exact variant matches the oracle; approximate stays within its stated error
- **Craft:** generic limiter traits (governor) · **Cookbook:** similar `capstones/rate_limiter_lab`

#### L03 · Bursts with a budget
60 min · `far` · branch
- **Adds:** token bucket
- **Goal:** burst-tolerant limits
- **Required first:** L01 (window-based limiting)
- **Used again in:** L04, L05
- **Outside FlowForge:** outbound call budgets
- **Test:** admitted count over any interval stays within the burst-plus-rate bound
- **Craft:** integer time arithmetic (governor) · **Cookbook:** similar `distributed/token_bucket`

#### L04 · Smooth the output
45 min · `far` · branch
- **Adds:** leaky bucket
- **Goal:** a pacing mode that evens out bursts
- **Required first:** L03 (token bucket)
- **Used again in:** no planned part yet
- **Outside FlowForge:** traffic shaping, send pacing
- **Test:** departures respect the stated spacing; over-capacity arrivals are refused
- **Craft:** queue-or-reject API (governor) · **Cookbook:** similar `capstones/rate_limiter_lab`

#### L05 · GCRA
60 min · `far` · branch
- **Adds:** generic cell rate algorithm
- **Goal:** the same limit with minimal per-key state
- **Required first:** L03 (token bucket)
- **Used again in:** N22, L06
- **Outside FlowForge:** gateway limiters
- **Test:** differential test against the L03 limiter on random arrivals, with differences explained
- **Craft:** compact state types (governor) · **Cookbook:** similar `capstones/rate_limiter_lab`

#### L06 · Limiter bench
90 min · `far` · branch
- **Adds:** comparative harness
- **Goal:** `limits compare` shows burst, fairness and memory per algorithm
- **Required first:** L02 (log vs approximate counter), L05 (generic cell rate algorithm)
- **Used again in:** L07, L08
- **Outside FlowForge:** choosing any limiter
- **Test:** identical seeded traffic gives reproducible tables; each algorithm's documented bound is asserted
- **Craft:** benchmarking (criterion) · **Cookbook:** similar `capstones/rate_limiter_lab`

#### L07 · Adaptive concurrency
120 min · `far` · branch
- **Adds:** adaptive concurrency limits
- **Goal:** the worker limit adjusts to observed latency and errors
- **Required first:** L06 (comparative harness), S05 (multi-resource admission)
- **Used again in:** no planned part yet
- **Outside FlowForge:** service-to-service concurrency control
- **Test:** in a simulated service with a changing capacity the limit stays in documented bounds and recovers; virtual time only
- **Craft:** feedback control state (tower, Netflix concurrency-limits by name) · **Cookbook:** similar `capstones/rate_limiter_lab`

#### L08 · Per-tenant fair queuing
90 min · `far` · branch
- **Adds:** fair queuing
- **Goal:** one noisy tenant cannot crowd out others
- **Required first:** L06 (comparative harness), S06 (fair scheduling) · **Helpful:** Q03 (aging)
- **Used again in:** L11
- **Outside FlowForge:** multi-tenant gateways
- **Test:** backlogged tenants receive weighted shares within a bound
- **Craft:** generics over keys (tokio)

#### L09 · Retry budgets
60 min · `far` · branch
- **Adds:** retry budgets
- **Goal:** retries across the whole system are capped relative to real traffic
- **Required first:** W11 (backoff with jitter), Q10 (admission control) · **Helpful:** C04 (time and choice as inputs (deterministic testing))
- **Used again in:** L10
- **Outside FlowForge:** preventing retry storms
- **Test:** retries never exceed the stated ratio in any window
- **Craft:** shared budget handles (tower) · **Cookbook:** similar `patterns/tower_service_layer`

#### L10 · Stop calling what is broken
60 min · `far` · branch
- **Adds:** circuit breaker
- **Goal:** a failing dependency is isolated and probed
- **Required first:** L09 (retry budgets)
- **Used again in:** L11
- **Outside FlowForge:** service meshes, database clients
- **Test:** transitions match a state table under virtual time
- **Craft:** state in middleware (tower) · **Cookbook:** similar `patterns/tower_service_layer`

#### L11 · Shed the right load
60 min · `far` · branch
- **Adds:** prioritised load shedding
- **Goal:** under overload, low-value work is refused first
- **Required first:** L08 (fair queuing), L10 (circuit breaker)
- **Used again in:** no planned part yet
- **Outside FlowForge:** survival under traffic spikes
- **Test:** in scripted overload, higher classes are never refused while lower ones are admitted
- **Craft:** policy composition (tower)

#### L capstone · Protect the platform
`far` · 3 milestones, 4 h · main-path parts needed run up to step 67. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| L-C1 Per-tenant limits | 90 min | selectable limiters and fair queuing on the API and admission | each limiter's bound under scripted traffic |
| L-C2 Overload drill | 90 min | adaptive limits, retry budgets, breakers and shedding together | priority classes and budgets hold in simulation |
| L-C3 Cluster-wide | 60 min | limits hold across instances during partitions | overshoot within bound |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| limit algorithms | L01, L02, L03, L04, L05 |
| comparison and adaptation | L06, L07 |
| fairness, budgets, breakers, shedding | L08, L09, L10, L11 |
| across nodes | N22 |
| admission and virtual time | Q10, C04 |
| carried in by those parts' Required chains | C01, C02, C03, C05, D01, D02, D03, N01, N02, N03, N04, N05, N07, N09, Q01, Q02, Q04, Q05, Q06, Q07, Q08, S02, S03, S05, S06, W09, W10, W11 |
| stages 01-08 | assumed done |

### G · Parsers and compilers (subject)

A typed workflow language that compiles to FlowForge.

#### G01 · Tokens
60 min · `far` · branch
- **Adds:** lexing
- **Goal:** `lex` shows tokens with positions for a workflow source file
- **Required first:** W08 (measure before optimising)
- **Used again in:** G02
- **Outside FlowForge:** any parser
- **Test:** token spans reassemble the source; random bytes never crash
- **Craft:** zero-copy lifetimes (oxc_parser) · **Cookbook:** related `text_and_parsing/zero_copy_lexer`

#### G02 · Expressions
90 min · `far` · branch
- **Adds:** precedence parsing
- **Goal:** conditions are written as expressions and evaluated
- **Required first:** G01 (lexing), W14 (conditional edges)
- **Used again in:** G03, E02
- **Outside FlowForge:** filter and rule languages
- **Test:** generated trees printed and reparsed are identical; evaluation matches a reference
- **Craft:** AST ownership (oxc) · **Cookbook:** similar `text_and_parsing/pratt_parser`

#### G03 · Workflow syntax
90 min · `far` · branch
- **Adds:** a front end over the same engine
- **Goal:** `compile` turns DSL source into the JSON schema
- **Required first:** G02 (precedence parsing)
- **Used again in:** G04
- **Outside FlowForge:** config and infrastructure languages
- **Test:** compiled output gets the same `order` as hand-written equivalents
- **Craft:** arena-allocated trees (oxc_allocator) · **Cookbook:** related `text_and_parsing/edge_dsl_parser`, `memory/bump_arena_allocator`

#### G04 · Diagnostics
60 min · `far` · branch
- **Adds:** error recovery with located diagnostics
- **Goal:** errors point at source and report several problems at once
- **Required first:** G03 (a front end over the same engine) · **Helpful:** AG05 (Who is really in the loop?)
- **Used again in:** G05, G11
- **Outside FlowForge:** compilers, linters, validators
- **Test:** each planted fault is reported at its position
- **Craft:** diagnostic design (biome_diagnostics, biome_rowan) · **Cookbook:** related `errors/diagnostics_spans`

#### G05 · Types
120 min · `far` · branch
- **Adds:** type checking
- **Goal:** ill-typed conditions and data wiring are rejected before running
- **Required first:** G04 (error recovery with located diagnostics)
- **Used again in:** G06, G10
- **Outside FlowForge:** schema and type checkers
- **Test:** well-typed generated programs never raise a type error at run time
- **Craft:** typed ids and visitors (rust-analyzer) · **Cookbook:** related `compilers_runtimes/system_f_bidirectional_typing`, `compilers_runtimes/hindley_milner_inference`

#### G06 · Bytecode VM
120 min · `far` · branch
- **Adds:** bytecode execution
- **Goal:** conditions run on a VM; `disasm` shows the code
- **Required first:** G05 (type checking)
- **Used again in:** G07
- **Outside FlowForge:** rule engines, query evaluators
- **Test:** VM agrees with the G02 evaluator on generated programs; fuel limit stops runaway code
- **Craft:** enum dispatch and layout (wasmtime, by name) · **Cookbook:** similar `text_and_parsing/bytecode_vm`

#### G07 · Optimiser
120 min · `far` · branch
- **Adds:** compiler optimisation
- **Goal:** constant folding and dead-code removal, switchable
- **Required first:** G06 (bytecode execution)
- **Used again in:** G08
- **Outside FlowForge:** compilers and query planners
- **Test:** optimised and unoptimised results identical on generated programs
- **Craft:** pass design (cranelift) · **Cookbook:** related `compilers_runtimes/dataflow_analysis_framework`

#### G08 · One name, one assignment
120 min · `far` · branch
- **Adds:** SSA form
- **Goal:** an intermediate form that simplifies further optimisation, visible in `disasm`
- **Required first:** G07 (compiler optimisation)
- **Used again in:** G09
- **Outside FlowForge:** compilers, JITs
- **Test:** results identical; form check passes on generated programs
- **Craft:** arena-indexed IR (cranelift) · **Cookbook:** related `compilers_runtimes/ssa_register_allocation`

#### G09 · Native code
120 min · `far` · branch
- **Adds:** a native backend (register allocation or Cranelift)
- **Goal:** optional compiled execution of conditions
- **Required first:** G08 (SSA form)
- **Used again in:** no planned part yet
- **Outside FlowForge:** JIT engines
- **Test:** differential against the VM
- **Craft:** unsafe boundaries (cranelift, wasmtime) · **Cookbook:** related `compilers_runtimes/ssa_register_allocation`

#### G10 · Incremental checking
120 min · `far` · branch
- **Adds:** query-based incrementality
- **Goal:** after an edit only affected results are recomputed, and `--stats` shows it
- **Required first:** G05 (type checking)
- **Used again in:** G11
- **Outside FlowForge:** IDE tooling, build systems
- **Test:** incremental output equals from-scratch; recompute counts are bounded
- **Craft:** memoisation APIs (salsa)

#### G11 · Editor support
120 min · `far` · branch
- **Adds:** LSP
- **Goal:** a language server gives diagnostics and go-to-definition
- **Required first:** G10 (query-based incrementality), G04 (error recovery with located diagnostics)
- **Used again in:** no planned part yet
- **Outside FlowForge:** editor tooling for any format
- **Test:** scripted JSON-RPC session over stdio
- **Craft:** async protocol servers (rust-analyzer, tower-lsp) · **Cookbook:** related `web/jsonrpc_graphql_dataloader`

#### G capstone · The FlowForge language
`far` · 3 milestones, 5 h · main-path parts needed run up to step 24. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| G-C1 Compile and check | 90 min | source with good diagnostics and types compiles to the engine's schema | round trips and planted errors |
| G-C2 Execute conditions | 120 min | conditions run on the VM and optionally natively, with optimisation | all modes agree |
| G-C3 Author comfortably | 90 min | editor support with incremental checking | scripted editor session |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| lexing, parsing, syntax | G01, G02, G03 |
| diagnostics and types | G04, G05 |
| bytecode and optimisation | G06, G07 |
| intermediate form and native code | G08, G09 |
| incremental and editor | G10, G11 |
| conditions in workflows | W14 |
| carried in by those parts' Required chains | C01, C02, C03, W09 |
| stages 01-08 | assumed done |

### K · Storage engine (subject)

FlowForge's own store behind the D03 behaviour; the D03 suite must keep passing.

#### K01 · Records on disk you can trust
60 min · `far` · branch
- **Adds:** binary record framing with integrity checks
- **Goal:** `log dump` writes and reads binary records and reports exactly where damage starts
- **Required first:** W08 (measure before optimising)
- **Used again in:** K02
- **Outside FlowForge:** file formats, network protocols
- **Test:** flip or cut every byte of a sample file: damage is detected at the right record, with no crash
- **Craft:** bounds-checked byte parsing (bytes) · **Cookbook:** related `text_and_parsing/binary_format_parsing`, `language/bytes`

#### K02 · Own append log
90 min · `far` · branch
- **Adds:** write-ahead logging
- **Goal:** `--store native` keeps run history in FlowForge's own log, tolerating a torn tail
- **Required first:** D03 (recovery from durable state), K01 (binary record framing with integrity checks)
- **Used again in:** TR06, I05, N16, K03, K08, H03
- **Outside FlowForge:** databases, brokers
- **Test:** truncate or corrupt at every byte offset: recovery yields a committed prefix, never garbage
- **Craft:** unsafe-free binary parsing (bytes) · **Cookbook:** similar `os_and_storage/wal_append_log`

#### K03 · Checkpoints
90 min · `far` · branch
- **Adds:** checkpointing
- **Goal:** startup time depends on live state, not total history
- **Required first:** K02 (write-ahead logging)
- **Used again in:** J06, N18, K04, K05
- **Outside FlowForge:** databases, game saves
- **Test:** state before and after checkpoint plus crash sweep is equal; records read at start are counted, not timed
- **Craft:** atomic file replace (tempfile) · **Cookbook:** related `os_and_storage/file_locking_atomic_replace`, `database/logging_schemes_group_commit`

#### K04 · Recovery with undo
120 min · `far` · branch
- **Adds:** ARIES basics
- **Goal:** multi-step state changes are all-or-nothing across crashes, including crashes during recovery
- **Required first:** K03 (checkpointing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** any transactional store
- **Test:** crash sweep including repeated crashes in recovery
- **Craft:** invariants in types (sled) · **Cookbook:** similar `database/aries_recovery`

#### K05 · LSM tables
120 min · `far` · branch
- **Adds:** log-structured tables
- **Goal:** history larger than memory stays queryable
- **Required first:** K03 (checkpointing)
- **Used again in:** K06
- **Outside FlowForge:** write-heavy stores
- **Test:** differential test against an in-test map across restarts
- **Craft:** iterator merging (RocksDB docs, mini-lsm) · **Cookbook:** similar `capstones/mini_kv_store`

#### K06 · Compaction
90 min · `far` · branch
- **Adds:** compaction
- **Goal:** space and read cost stay bounded; `store stats` reports them
- **Required first:** K05 (log-structured tables)
- **Used again in:** K07, K10
- **Outside FlowForge:** LSM stores, log cleanup
- **Test:** results unchanged by compaction; reported amplification within bounds
- **Craft:** background work and ownership (fjall, mini-lsm) · **Cookbook:** similar `database/lsm_compaction_strategies`

#### K07 · Skip files that cannot match
60 min · `far` · branch
- **Adds:** Bloom filters
- **Goal:** lookups skip files that cannot hold the key
- **Required first:** K06 (compaction)
- **Used again in:** no planned part yet
- **Outside FlowForge:** caches, databases, deduplication
- **Test:** never a false negative; files read are counted and fall on absent keys
- **Craft:** bit sets behind an API (mini-lsm) · **Cookbook:** similar `search_vectors/bloom_filter`

#### K08 · Ordered index
120 min · `far` · branch
- **Adds:** B+tree
- **Goal:** range queries over runs by time and id use an on-disk index
- **Required first:** K02 (write-ahead logging)
- **Used again in:** K09
- **Outside FlowForge:** database indexes, filesystems
- **Test:** random operations match an in-test ordered map; structural check command passes
- **Craft:** arena and page ids (redb) · **Cookbook:** related `database/slotted_pages_heap_file`; similar `data_structures/bplus_tree`

#### K09 · Buffer pool
90 min · `far` · branch
- **Adds:** page caching
- **Goal:** a memory cap is respected on large indexes
- **Required first:** K08 (B+tree)
- **Used again in:** V09
- **Outside FlowForge:** databases, page caches
- **Test:** pinned and resident page counts from stats never exceed the cap; results unaffected
- **Craft:** RAII pin guards (sled, redb) · **Cookbook:** related `database/lru_k_replacer_disk_scheduler`; similar `database/buffer_pool_clock`

#### K10 · Snapshot reads
120 min · `far` · branch
- **Adds:** MVCC
- **Goal:** status queries see one consistent snapshot while runs write
- **Required first:** K06 (compaction)
- **Used again in:** K11
- **Outside FlowForge:** PostgreSQL-style databases
- **Test:** history checker finds no mixed snapshots; old versions are reclaimed
- **Craft:** epoch-style reclamation (crossbeam) · **Cookbook:** similar `database/mvcc_snapshot_isolation`

#### K11 · Conflicting writers: wait
90 min · `far` · branch
- **Adds:** two-phase locking
- **Goal:** concurrent state transitions are serialisable by making writers wait
- **Required first:** K10 (MVCC)
- **Used again in:** K12
- **Outside FlowForge:** relational databases
- **Test:** recorded histories pass a serialisability check in the test; no permanent deadlock
- **Craft:** lock ordering (parking_lot) · **Cookbook:** similar `database/lock_manager_deadlock`

#### K12 · Conflicting writers: validate
90 min · `far` · branch
- **Adds:** optimistic concurrency control
- **Goal:** the same guarantee without holding locks, compared with the previous mode
- **Required first:** K11 (two-phase locking)
- **Used again in:** no planned part yet
- **Outside FlowForge:** in-memory databases, version checks
- **Test:** same history checks; abort counts reported under both
- **Craft:** retry loops in APIs (sled) · **Cookbook:** similar `database/timestamp_ordering_occ`

#### K capstone · FlowForge's own database file
`far` · 3 milestones, 5.5 h · main-path parts needed run up to step 16. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| K-C1 Swap the store | 120 min | the native store passes the whole durable-state suite including crash sweeps | parity and corruption sweep |
| K-C2 Beyond memory | 120 min | large history with compaction, filters, an index and a memory cap | differential tests and stats bounds |
| K-C3 Concurrent access | 90 min | snapshot reads and both writer modes under load | history checks |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| record format and log | K01, K02 |
| checkpoint and undo | K03, K04 |
| tables, compaction, filters | K05, K06, K07 |
| index and page cache | K08, K09 |
| snapshots and writers | K10, K11, K12 |
| transactions and recovery | D02, D03 |
| carried in by those parts' Required chains | C01, C02, C03, D01 |
| stages 01-08 | assumed done |

### E · Databases: query and execution (subject)

`flowforge query` over execution state. Snapshots and concurrent writers are in K10 to K12; basic transactions in D02.

#### E01 · Filters over runs
60 min · `far` · branch
- **Adds:** a query surface
- **Goal:** `query` filters runs and tasks by simple conditions
- **Required first:** D03 (recovery from durable state)
- **Used again in:** J05, E02
- **Outside FlowForge:** admin filters, log queries
- **Test:** compare with filtering the JSON export in the test
- **Craft:** borrowing query inputs, lifetimes (jiff or ripgrep globset)

#### E02 · Parse a SELECT
90 min · `far` · branch
- **Adds:** SQL parsing
- **Goal:** a SELECT subset is parsed with located errors
- **Required first:** E01 (a query surface), G02 (precedence parsing)
- **Used again in:** E03
- **Outside FlowForge:** any query or rule language
- **Test:** print-then-parse round trip; bad input never crashes
- **Craft:** recursive AST design (sqlparser-rs) · **Cookbook:** similar `capstones/tiny_sql_engine`

#### E03 · Bind names and types
60 min · `far` · branch
- **Adds:** semantic binding
- **Goal:** names and types are resolved before execution
- **Required first:** E02 (SQL parsing)
- **Used again in:** E04
- **Outside FlowForge:** compilers, ORMs
- **Test:** every unknown or mistyped name is reported at its position
- **Craft:** typed ids (DataFusion) · **Cookbook:** similar `capstones/tiny_sql_engine`

#### E04 · Iterator execution
120 min · `far` · branch
- **Adds:** volcano operators
- **Goal:** joins and aggregates over runs, with `explain`
- **Required first:** E03 (semantic binding)
- **Used again in:** E05, E06
- **Outside FlowForge:** query engines, data tools
- **Test:** differential test against a naive evaluator or SQLite
- **Craft:** trait objects in operator trees (DataFusion) · **Cookbook:** related `database/relational_algebra_engine`; similar `capstones/tiny_sql_engine`

#### E05 · Vectorized mode
120 min · `far` · branch
- **Adds:** vectorized execution
- **Goal:** a second execution mode over batches
- **Required first:** E04 (volcano operators), P03 (atomics and memory ordering)
- **Used again in:** no planned part yet
- **Outside FlowForge:** analytics engines
- **Test:** both modes give identical results on random queries
- **Craft:** data layout (arrow-rs) · **Cookbook:** similar `database/volcano_vectorized_push`

#### E06 · Cost-based planning
120 min · `far` · branch
- **Adds:** cost-based join ordering
- **Goal:** join order chosen from statistics; `explain` shows estimates
- **Required first:** E04 (volcano operators), V16 (HyperLogLog)
- **Used again in:** no planned part yet
- **Outside FlowForge:** reading EXPLAIN output
- **Test:** results independent of plan; chosen cost matches exhaustive search on small joins
- **Craft:** memo structures (DataFusion) · **Cookbook:** related `database/cascades_memo_optimizer`; similar `capstones/query_optimizer`

#### E capstone · Query your runs
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 29. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| E-C1 A useful SQL subset | 120 min | real operational questions answered with joins and aggregates and explained | agrees with a reference engine |
| E-C2 Faster plans | 90 min | vectorized and cost-chosen plans give the same answers | differential random queries |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| filters | E01 |
| parsing and binding | E02, E03 |
| execution | E04, E05 |
| planning | E06 |
| expressions | G02 |
| statistics | V16, V17 |
| carried in by those parts' Required chains | C01, C02, C03, C05, C10, D01, D02, D03, G01, P01, P03, W09, W14 |
| stages 01-08 | assumed done |

### H · Search (subject)

Search over task logs.

#### H01 · Grep the logs
45 min · `far` · branch
- **Adds:** a correct baseline
- **Goal:** tasks produce logs; `search <text>` finds matches by scanning
- **Required first:** D03 (recovery from durable state)
- **Used again in:** H02, H04, V01
- **Outside FlowForge:** log search
- **Test:** hits equal an in-test substring scan
- **Craft:** byte search (memchr, ripgrep) · **Cookbook:** related `algorithms/string_algos`

#### H02 · Trigram index
120 min · `far` · branch
- **Adds:** trigram inverted index
- **Goal:** the same answers from an index, with candidate counts in `--stats`
- **Required first:** H01 (a correct baseline)
- **Used again in:** H03, H05, H08
- **Outside FlowForge:** code search, LIKE acceleration
- **Test:** always equal to the scan; fewer candidates verified on selective queries
- **Craft:** literal extraction (regex-automata) · **Cookbook:** similar `search_vectors/trigram_index`

#### H03 · Smaller postings
90 min · `far` · branch
- **Adds:** posting-list compression
- **Goal:** the index persists compactly
- **Required first:** H02 (trigram inverted index), K02 (write-ahead logging)
- **Used again in:** no planned part yet
- **Outside FlowForge:** search engines, analytics bitmaps
- **Test:** round trip and identical results; size reported against raw
- **Craft:** bit-level APIs (roaring-rs) · **Cookbook:** related `data_structures/bitset_rank_select_roaring`

#### H04 · What counts as a word
45 min · `far` · branch
- **Adds:** text tokenisation
- **Goal:** `terms <log>` shows how log text is split and normalised
- **Required first:** H01 (a correct baseline)
- **Used again in:** H05
- **Outside FlowForge:** every search engine
- **Test:** table of paths, ids, timestamps and Unicode; query and index use the same rules
- **Craft:** `char` and Unicode boundaries (unicode-segmentation) · **Cookbook:** related `std_tour/std_char_ascii_unicode`

#### H05 · Ranked results
90 min · `far` · branch
- **Adds:** BM25
- **Goal:** results are ranked by relevance
- **Required first:** H02 (trigram inverted index), H04 (text tokenisation)
- **Used again in:** H06, V10
- **Outside FlowForge:** document search
- **Test:** scores match a reference computation in the test; ties stable
- **Craft:** float ordering (tantivy) · **Cookbook:** similar `capstones/search_engine`

#### H06 · A compact term dictionary
90 min · `far` · branch
- **Adds:** finite state transducers
- **Goal:** the term dictionary is compact and supports prefix lookup
- **Required first:** H05 (BM25)
- **Used again in:** H07
- **Outside FlowForge:** autocomplete, search engines
- **Test:** same lookups as an in-test map; size reported
- **Craft:** builder then frozen types (fst) · **Cookbook:** related `data_structures/trie_radix`

#### H07 · Forgive typos
90 min · `far` · branch
- **Adds:** Levenshtein automata
- **Goal:** search tolerates small typos
- **Required first:** H06 (finite state transducers)
- **Used again in:** no planned part yet
- **Outside FlowForge:** spell-tolerant search
- **Test:** matches equal brute-force distance filtering
- **Craft:** automata traits (fst, regex-automata)

#### H08 · Suffix arrays
90 min · `far` · branch
- **Adds:** suffix arrays
- **Goal:** an alternative index mode, compared
- **Required first:** H02 (trigram inverted index)
- **Used again in:** no planned part yet
- **Outside FlowForge:** substring and genome search
- **Test:** same hits as scan; comparison report
- **Craft:** slices and lifetimes (BurntSushi suffix, by name) · **Cookbook:** related `algorithms/string_algos`

#### H capstone · Search every log
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 16. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| H-C1 Indexed, ranked search | 120 min | indexed substring and ranked word search over run history that survives restart | equals scan; ranking matches reference |
| H-C2 Forgiving and compact | 90 min | typo tolerance and compact storage | brute-force parity and size report |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| baseline and substring index | H01, H02, H08 |
| compact storage | H03 |
| words and ranking | H04, H05 |
| dictionary and typos | H06, H07 |
| own log format | K02 |
| carried in by those parts' Required chains | C01, C02, C03, D01, D02, D03, K01 |
| stages 01-08 | assumed done |

### V · Vectors and discovery (subject)

"Find similar failures", duplicates, suggestions.

#### V01 · From a log to numbers
60 min · `far` · branch
- **Adds:** representing text as vectors
- **Goal:** `vectorize` turns a task log into a fixed-length vector locally, with no external service
- **Required first:** H01 (a correct baseline)
- **Used again in:** V02
- **Outside FlowForge:** search, spam filtering, recommendations
- **Test:** same log gives the same vector; constructed similar logs score closer than unrelated ones under a stated metric
- **Craft:** hashing and normalisation (std)

#### V02 · Similar failures, exactly
90 min · `far` · branch
- **Adds:** vectors, metrics, exact kNN
- **Goal:** `similar <failed task>` returns nearest past failures from locally computed vectors
- **Required first:** V01 (representing text as vectors)
- **Used again in:** V03, V04
- **Outside FlowForge:** recommendations, semantic search
- **Test:** matches an in-test brute force; self is nearest
- **Craft:** float handling and total order (std) · **Cookbook:** similar `search_vectors/knn_bruteforce`

#### V03 · Recall harness
60 min · `far` · branch
- **Adds:** recall versus cost
- **Goal:** `ann eval` reports recall at k and work done per query
- **Required first:** V02 (vectors, metrics, exact kNN)
- **Used again in:** V05, V06, V08
- **Outside FlowForge:** choosing any vector database
- **Test:** exact mode scores 1.0; counts of distance computations are deterministic
- **Craft:** reproducible benchmarks (criterion) · **Cookbook:** related `search_vectors/ann_evaluation_harness`

#### V04 · Distance kernels
90 min · `far` · branch
- **Adds:** SIMD kernels
- **Goal:** faster distance computation with a scalar fallback
- **Required first:** V02 (vectors, metrics, exact kNN), P06 (SIMD)
- **Used again in:** no planned part yet
- **Outside FlowForge:** ML inference, image processing
- **Test:** kernels agree with scalar within a stated tolerance on all lengths
- **Craft:** unsafe and feature detection (memchr) · **Cookbook:** similar `search_vectors/vector_distance_kernels_simd`

#### V05 · LSH
90 min · `far` · branch
- **Adds:** locality-sensitive hashing
- **Goal:** first approximate mode
- **Required first:** V03 (recall versus cost)
- **Used again in:** no planned part yet
- **Outside FlowForge:** near-duplicate lookup
- **Test:** recall floor on seeded data; fewer comparisons than exact
- **Craft:** seeded randomness (rand) · **Cookbook:** related `data_structures/minhash_simhash_lsh`

#### V06 · IVF
90 min · `far` · branch
- **Adds:** k-means partitioning
- **Goal:** partitioned search with a probe setting
- **Required first:** V03 (recall versus cost)
- **Used again in:** V07
- **Outside FlowForge:** vector databases
- **Test:** probing everything equals exact; recall never falls as probes rise
- **Craft:** parallel iterators (rayon) · **Cookbook:** similar `search_vectors/ivf_flat`

#### V07 · Quantization
120 min · `far` · branch
- **Adds:** vector quantization
- **Goal:** compressed vectors with rescoring
- **Required first:** V06 (k-means partitioning)
- **Used again in:** V09
- **Outside FlowForge:** fitting large indexes in memory
- **Test:** memory report; recall floors; full rescore equals exact on candidates
- **Craft:** compact layouts (qdrant) · **Cookbook:** similar `search_vectors/product_quantization`, `search_vectors/scalar_binary_quantization_rescoring`

#### V08 · HNSW
120 min · `far` · branch
- **Adds:** HNSW
- **Goal:** graph-based approximate search
- **Required first:** V03 (recall versus cost), W08 (measure before optimising)
- **Used again in:** V09, V10
- **Outside FlowForge:** vector databases, retrieval for LLMs
- **Test:** recall floor on seeded data; structural check; deterministic under a seed
- **Craft:** index graphs at scale (hnswlib by name, qdrant) · **Cookbook:** similar `search_vectors/hnsw_lite`

#### V09 · Disk graph
120 min · `far` · branch
- **Adds:** DiskANN/Vamana
- **Goal:** search with a bounded memory budget
- **Required first:** V08 (HNSW), K09 (page caching) · **Helpful:** V07 (vector quantization)
- **Used again in:** no planned part yet
- **Outside FlowForge:** billion-scale retrieval
- **Test:** node reads reported and bounded; recall floor
- **Craft:** async or mapped I/O (DiskANN repo, by name) · **Cookbook:** similar `search_vectors/vamana_diskann_lite`

#### V10 · Keywords and vectors together
60 min · `far` · branch
- **Adds:** rank fusion
- **Goal:** one ranking from keyword and vector results
- **Required first:** V08 (HNSW), H05 (BM25)
- **Used again in:** V11, V13
- **Outside FlowForge:** retrieval for LLMs, product search
- **Test:** matches reference arithmetic on fixed inputs
- **Craft:** combining iterators (std) · **Cookbook:** similar `search_vectors/filtered_ann_hybrid_search`

#### V11 · Search with filters
90 min · `far` · branch
- **Adds:** filtered ANN
- **Goal:** vector search that respects filters
- **Required first:** V10 (rank fusion)
- **Used again in:** no planned part yet
- **Outside FlowForge:** e-commerce and tenant search
- **Test:** filters never violated; recall floor across selectivities
- **Craft:** query builder API (qdrant) · **Cookbook:** similar `search_vectors/filtered_ann_hybrid_search`

#### V12 · Duplicate workflows
90 min · `far` · branch
- **Adds:** near-duplicate sketches
- **Goal:** `duplicates` flags near-identical definitions and logs
- **Required first:** W07 (determinism and invariant testing)
- **Used again in:** no planned part yet
- **Outside FlowForge:** plagiarism and duplicate detection
- **Test:** planted near-duplicates found; estimates within error bounds of exact similarity
- **Craft:** hashing traits (std Hash) · **Cookbook:** similar `data_structures/minhash_simhash_lsh`

#### V13 · Group similar failures
90 min · `far` · branch
- **Adds:** clustering
- **Goal:** clusters of similar failures, also joined with lineage
- **Required first:** V10 (rank fusion), J02 (data lineage)
- **Used again in:** V14, V15
- **Outside FlowForge:** alert grouping
- **Test:** planted clusters recovered on seeded data
- **Craft:** pipeline APIs (linfa, by name)

#### V14 · What usually comes next
60 min · `far` · branch
- **Adds:** recommendation from similarity
- **Goal:** suggests likely next tasks while authoring
- **Required first:** V13 (clustering)
- **Used again in:** no planned part yet
- **Outside FlowForge:** recommendations
- **Test:** planted patterns are suggested; nothing already present is suggested
- **Craft:** trait for rankers (std)

#### V15 · This run looks unusual
60 min · `far` · branch
- **Adds:** anomaly detection
- **Goal:** flags unusual execution traces
- **Required first:** V13 (clustering)
- **Used again in:** no planned part yet
- **Outside FlowForge:** monitoring and fraud
- **Test:** planted anomalies flagged on seeded data with a bounded false-positive count
- **Craft:** numeric summaries (std)

#### V16 · Distinct counts in fixed memory
60 min · `far` · branch
- **Adds:** HyperLogLog
- **Goal:** `stats` reports distinct values in fixed memory
- **Required first:** D03 (recovery from durable state)
- **Used again in:** E06, V17
- **Outside FlowForge:** analytics, unique visitors
- **Test:** error within bound against exact counts; memory fixed
- **Craft:** mergeable types (std) · **Cookbook:** similar `data_structures/hyperloglog`

#### V17 · Frequent items in fixed memory
60 min · `far` · branch
- **Adds:** Count-Min sketch
- **Goal:** reports the heaviest items in fixed memory
- **Required first:** V16 (HyperLogLog)
- **Used again in:** no planned part yet
- **Outside FlowForge:** hot-key detection
- **Test:** never under-counts; error within bound
- **Craft:** generic hashing (std Hash) · **Cookbook:** similar `data_structures/count_min_sketch`

#### V capstone · Find similar failures
`far` · 3 milestones, 5 h · main-path parts needed run up to step 29. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| V-C1 Similar-failure service | 90 min | from a failed task, return past failures, exact and approximate, with a recall report | recall floors on seeded data |
| V-C2 Scale it | 120 min | compressed, disk-bounded and filtered modes | budgets and filters hold |
| V-C3 Discovery | 90 min | duplicates, clusters, suggestions, unusual runs and fixed-memory statistics | planted patterns recovered |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| vectors and exact search | V01, V02 |
| evaluation and kernels | V03, V04 |
| approximate indexes | V05, V06, V07, V08, V09 |
| hybrid and filtered | V10, V11 |
| discovery | V12, V13, V14, V15 |
| statistics | V16, V17 |
| keyword ranking and lineage | H05, J02 |
| carried in by those parts' Required chains | AG01, C01, C02, C03, C05, C10, D01, D02, D03, H01, H02, H04, J01, K01, K02, K08, K09, P01, P03, P04, P06 |
| stages 01-08 | assumed done |

### P · Performance (subject)

A faster engine; behaviour suites must pass unchanged.

#### P01 · Benchmark suite
60 min · `far` · branch
- **Adds:** repeatable measurement
- **Goal:** `bench` runs fixed workloads and writes a report
- **Required first:** W08 (measure before optimising), C03 (bounded concurrency)
- **Used again in:** O06, M05, P02, P03, P06
- **Outside FlowForge:** every optimisation effort
- **Test:** report schema and workload determinism; no thresholds in tests
- **Craft:** criterion and release profiles (ripgrep) · **Cookbook:** related `perf/bench_harness`

#### P02 · Read a profile
60 min · `far` · branch
- **Adds:** profiling
- **Goal:** a written finding from a profile of one benchmark, naming the hottest path and a prediction
- **Required first:** P01 (repeatable measurement)
- **Used again in:** no planned part yet
- **Outside FlowForge:** every performance investigation
- **Test:** the report names the command and build profile; the prediction is checked by a later benchmark run
- **Craft:** release profiles with symbols (ripgrep, inferno) · **Cookbook:** related `perf/bench_harness`, `perf/perf_checklist`

#### P03 · Cheap live counters
90 min · `far` · branch
- **Adds:** atomics and memory ordering
- **Goal:** hot-path statistics without contention
- **Required first:** C10 (shared state), P01 (repeatable measurement)
- **Used again in:** E05, P04
- **Outside FlowForge:** telemetry in hot services
- **Test:** totals reconcile with traces; loom-style check mode passes
- **Craft:** atomics (tokio, loom) · **Cookbook:** related `concurrency/atomics_ordering`, `perf/false_sharing`

#### P04 · Unsafe with a contract
90 min · `far` · branch
- **Adds:** unsafe boundaries
- **Goal:** one small `unsafe` building block with written safety conditions, exercised by a check mode
- **Required first:** P03 (atomics and memory ordering)
- **Used again in:** P05, P06
- **Outside FlowForge:** FFI and high-performance libraries
- **Test:** tests run under Miri where possible; misuse is impossible through the safe API
- **Craft:** safety comments and Miri (std, crossbeam) · **Cookbook:** related `unsafe_ffi/unsafe_basics`, `unsafe_ffi/maybeuninit_fixedvec`

#### P05 · Lock-free hand-off
120 min · `far` · branch
- **Adds:** lock-free queues
- **Goal:** an optional lock-free ready queue
- **Required first:** P04 (unsafe boundaries), C11 (systematic concurrency testing) · **Helpful:** Q02 (backpressure)
- **Used again in:** no planned part yet
- **Outside FlowForge:** trading and logging pipelines
- **Test:** C11 and Q02 suites pass; interleaving checks on small bounds
- **Craft:** unsafe boundaries (crossbeam) · **Cookbook:** related `concurrency/spsc_ring_buffer`, `concurrency/epoch_hazard_pointer_reclamation`; similar `concurrency/mpmc_bounded_queue`

#### P06 · A SIMD hot path
90 min · `far` · branch
- **Adds:** SIMD
- **Goal:** one measured hot loop vectorised, with fallback
- **Required first:** P01 (repeatable measurement), P04 (unsafe boundaries)
- **Used again in:** V04
- **Outside FlowForge:** codecs, analytics kernels
- **Test:** identical output on all lengths and alignments
- **Craft:** `std::arch` safety (memchr) · **Cookbook:** related `perf/simd_intrinsics`

#### P capstone · Make it fast, prove it
`far` · 2 milestones, 3 h · main-path parts needed run up to step 32. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| P-C1 Baseline and diagnosis | 60 min | a benchmark report and a profile finding for the current engine | reproducible workloads and a stated prediction |
| P-C2 One justified optimisation at a time | 120 min | apply chosen optimisations behind flags with before and after reports | behaviour suites unchanged |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| measurement and profiling | P01, P02 |
| counters, unsafe, hand-off, SIMD | P03, P04, P05, P06 |
| memory | M05, M06 |
| interleaving checks | C11 |
| carried in by those parts' Required chains | C01, C02, C03, C04, C05, C06, C07, C08, C10 |
| stages 01-08 | assumed done |

### AR · Recursion and backtracking (algorithm)

Questions about nested and combinatorial structure in workflows.

**About the six algorithm series and MX.** The series name tells you which family you are practising; the parts
state only behaviour and tests. Finding the method, the representation and the argument that it is right is the
exercise. Parts that have no honest FlowForge feature are not invented: grid and board puzzles, string-puzzle and
game variants of dynamic programming, digit and range-of-numbers counting, classic backtracking puzzles (queens,
sudoku), bit riddles and XOR tricks, weighted shortest routes and cheapest-network questions have no natural home here.
Leave them to the anneal exercises (that repository was not opened for this plan, so check what it already holds).

#### AR01 · Show me everything under this
45 min · `next` · branch
- **Goal:** `tree <task>` prints what a task requires as a nested view, marking repeats, and survives very deep input
- **Required first:** W08 (measure before optimising)
- **Used again in:** no planned part yet
- **Outside FlowForge:** file trees, dependency printers
- **Test:** matches a reference built by the test on small inputs; output bounded on heavily shared graphs; 20k-deep input does not crash
- **Craft:** formatting design (std fmt)

#### AR02 · Workflows inside workflows
60 min · `next` · branch
- **Goal:** a definition can include others; self-inclusion in any form is a clear error
- **Required first:** W08 (measure before optimising)
- **Used again in:** AR03
- **Outside FlowForge:** config includes, templates
- **Test:** generated include trees flatten to the test's expansion; ids stay unique
- **Craft:** owned trees and `Box` (serde)

#### AR03 · Every variant of a job
60 min · `next` · branch
- **Goal:** parameter lists and exclusion rules expand to all allowed variants
- **Required first:** AR02 (Workflows inside workflows)
- **Used again in:** AR04
- **Outside FlowForge:** CI build matrices, test parameters
- **Test:** count and set equal a naive reference in the test; order stable; huge expansions refused up front
- **Craft:** lazy iterators (itertools)

#### AR04 · Every way to run it
90 min · `next` · branch
- **Goal:** `orders --limit k` lists distinct valid execution orders of small workflows
- **Required first:** AR03 (Every variant of a job)
- **Used again in:** AR05
- **Outside FlowForge:** test interleaving generation
- **Test:** each order valid and unique; for tiny inputs the set equals brute force; the limit stops work early, shown by a work counter
- **Craft:** iterator state (itertools)

#### AR05 · Pick a legal set
120 min · `next` · branch
- **Goal:** optional tasks with requires and excludes rules: find a legal selection or report that none exists
- **Required first:** AR04 (Every way to run it)
- **Used again in:** AR06
- **Outside FlowForge:** package resolution, feature flags
- **Test:** verdict equals exhaustive check on small cases; selections satisfy every rule; work counter under budget on constructed cases
- **Craft:** explaining failures (varisat, by name)

#### AR06 · Make it fit
120 min · `far` · branch
- **Goal:** find an assignment of tasks to limited workers under constraints, or prove none exists, within a work budget
- **Required first:** AR05 (Pick a legal set), S05 (multi-resource admission)
- **Used again in:** MX04, MX06
- **Outside FlowForge:** timetabling, seating, rostering
- **Test:** agrees with brute force; a budget overrun is reported as unknown
- **Craft:** explicit budgets in APIs (std)

#### AR capstone · Planner for optional and nested work
`far` · 2 milestones, 3 h · main-path parts needed run up to step 38. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| AR-C1 Expand | 60 min | nested groups and variants expand and display correctly | reference comparison |
| AR-C2 Choose and place | 120 min | select a legal task set and fit it to limited workers, or say why not | brute-force agreement under budgets |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| nested display and inclusion | AR01, AR02 |
| variants and all orders | AR03, AR04 |
| legal selection and fitting | AR05, AR06 |
| carried in by those parts' Required chains | C01, C02, C03, C04, S02, S03, S05 |
| stages 01-08 | assumed done |

### AG · Graphs (algorithm)

Questions about the dependency structure of a workflow.

#### AG01 · What does this change touch?
60 min · `next` · main step 20
- **Goal:** `impact <task>` lists everything affected if it changes or fails
- **Required first:** W08 (measure before optimising)
- **Used again in:** J02, AG02, AG04, AG05, AG06, AG08, AG09, AB03, MX02, MX05
- **Outside FlowForge:** build invalidation, incident blast radius
- **Test:** equals a naive oracle on random workflows; stable output
- **Craft:** iterators versus collect (std)

#### AG02 · Only what I need
45 min · `next` · main step 34
- **Goal:** `run --only <task>` runs exactly what the target requires
- **Required first:** AG01 (What does this change touch?), C01 (async execution)
- **Used again in:** no planned part yet
- **Outside FlowForge:** make and build targets
- **Test:** ran set equals the oracle; run invariants hold
- **Craft:** options as types (cargo)

#### AG03 · Waves
60 min · `next` · branch
- **Goal:** `plan --waves` shows the earliest wave in which each task could run with unlimited workers
- **Required first:** W08 (measure before optimising)
- **Used again in:** no planned part yet
- **Outside FlowForge:** release trains, course planning
- **Test:** each task is in the earliest wave consistent with its requirements
- **Craft:** grouped result types (std)

#### AG04 · How are these two related?
60 min · `next` · branch
- **Goal:** `why a b` prints a shortest chain of requirements linking two tasks, or says none
- **Required first:** AG01 (What does this change touch?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** "why is this package installed"
- **Test:** chain is real; length equals the oracle minimum; stated tie rule
- **Craft:** `Option` in results (petgraph)

#### AG05 · Who is really in the loop?
90 min · `next` · branch
- **Goal:** the cycle error names the tasks that form loops, not those merely stuck behind one
- **Required first:** AG01 (What does this change touch?)
- **Used again in:** G04, AG10
- **Outside FlowForge:** circular imports, deadlock reports
- **Test:** planted loops: every real loop group reported with no bystanders; 20k scale
- **Craft:** diagnostics that point at cause (biome_diagnostics)

#### AG06 · Links that say nothing new
90 min · `next` · branch
- **Goal:** `lint` reports requirements already implied by others
- **Required first:** AG01 (What does this change touch?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** dependency hygiene, permission inheritance
- **Test:** removing every reported item keeps all must-happen-before pairs, and nothing more can be removed
- **Craft:** lint report design (clippy)

#### AG07 · Separate pipelines in one file
45 min · `next` · branch
- **Goal:** `split` reports groups of tasks that never interact
- **Required first:** W08 (measure before optimising)
- **Used again in:** AG08
- **Outside FlowForge:** sharding, module boundaries
- **Test:** groups partition the tasks; none shares a link; none splits further
- **Craft:** returning borrowed groups (std)

#### AG08 · Single points of failure
120 min · `far` · branch
- **Goal:** `risk` reports tasks that every route from a start to a chosen target must pass through
- **Required first:** AG01 (What does this change touch?), AG07 (Separate pipelines in one file)
- **Used again in:** no planned part yet
- **Outside FlowForge:** network resilience, program analysis
- **Test:** removing a reported task cuts all routes; removing any unreported one does not
- **Craft:** analysis over borrowed data (rustc, by name)

#### AG09 · The widest moment
120 min · `far` · branch
- **Goal:** `width` reports the most tasks that could ever run at once, with a witness set
- **Required first:** AG01 (What does this change touch?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** capacity planning
- **Test:** witnesses are mutually independent; size equals brute force on small inputs
- **Craft:** witness-carrying results (petgraph)

#### AG10 · Stay valid while editing
120 min · `far` · branch
- **Goal:** an edit session accepts or rejects each new dependency without redoing all work, shown by a work counter
- **Required first:** W16 (map-style expansion), AG05 (Who is really in the loop?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** spreadsheets, incremental build graphs
- **Test:** verdicts equal full revalidation after any edit sequence; work counter under budget
- **Craft:** invariant-keeping mutation APIs (petgraph)

#### AG capstone · Dependency analyser
`far` · 2 milestones, 3 h · main-path parts needed run up to step 34. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| AG-C1 One analysis report | 90 min | impact, explanation, loops, redundant links and groups in one command | each section equals its oracle at 20k tasks |
| AG-C2 Risk and capacity | 90 min | single points of failure and widest moment, kept valid during edits | brute-force agreement and work budgets |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| impact, target runs, waves, explanation | AG01, AG02, AG03, AG04 |
| loops, redundancy, groups | AG05, AG06, AG07 |
| risk, width, editing | AG08, AG09, AG10 |
| carried in by those parts' Required chains | C01, C02, C03, W09, W14, W15, W16 |
| stages 01-08 | assumed done |

### AD · Dynamic programming (algorithm)

Optimisation and comparison questions about workflows.

#### AD01 · Did you mean?
45 min · `next` · main step 23
- **Goal:** unknown dependency errors suggest the closest existing id when one is close
- **Required first:** W04 (semantic validation)
- **Used again in:** no planned part yet
- **Outside FlowForge:** compiler and CLI hints
- **Test:** suggestion equals the test's reference under a stated distance rule; none when nothing is close
- **Craft:** `str` versus `chars` (strsim)

#### AD02 · How much slack?
60 min · `next` · main step 37
- **Goal:** `critical-path --slack` says how long each task can be delayed without delaying the whole
- **Required first:** S01 (critical-path analysis)
- **Used again in:** AD06, MX05
- **Outside FlowForge:** project planning
- **Test:** delaying by the slack keeps the total; one unit more raises it, checked by oracle
- **Craft:** reusing analysis results (petgraph)

#### AD03 · What changed between versions?
90 min · `next` · branch
- **Goal:** `diff a b` reports the smallest set of step-list edits between two definitions
- **Required first:** W08 (measure before optimising)
- **Used again in:** no planned part yet
- **Outside FlowForge:** code review, sync tools
- **Test:** applying the edits to a gives b; edit count equals a reference on small inputs; large inputs finish under a work budget
- **Craft:** diff APIs (similar)

#### AD04 · The best use of a budget
90 min · `next` · branch
- **Goal:** optional tasks have value and cost; `choose --budget` picks the most valuable set for one worker
- **Required first:** W04 (semantic validation)
- **Used again in:** AD05, AD06, AD07, MX03
- **Outside FlowForge:** portfolio and sprint selection
- **Test:** within budget; value equals exhaustive search on small inputs; larger cases finish under a work budget
- **Craft:** numeric overflow discipline (std)

#### AD05 · Where to save progress
120 min · `far` · branch
- **Goal:** for a long chain with failure rate and save cost, propose where to checkpoint to minimise expected redone work
- **Required first:** AD04 (The best use of a budget), D03 (recovery from durable state)
- **Used again in:** no planned part yet
- **Outside FlowForge:** training jobs, long ETL
- **Test:** cost equals exhaustive search on short chains; never worse than saving everywhere or nowhere
- **Craft:** float comparison discipline (std)

#### AD06 · Spend the speed-up budget
120 min · `far` · branch
- **Goal:** on tree-shaped workflows, choose which tasks to accelerate from priced options
- **Required first:** AD02 (How much slack?), AD04 (The best use of a budget)
- **Used again in:** no planned part yet
- **Outside FlowForge:** performance budgeting
- **Test:** equals brute force on small trees; budget respected
- **Craft:** tree ownership (oxc, by name)

#### AD07 · The true best, when small
120 min · `far` · branch
- **Goal:** `schedule --exact` gives the shortest completion with N workers for small workflows and refuses larger ones
- **Required first:** S04 (heuristic scheduling), AD04 (The best use of a budget)
- **Used again in:** AY05, AB05
- **Outside FlowForge:** oracles for any heuristic
- **Test:** equals brute force; never above any heuristic
- **Craft:** size limits in types (std)

#### AD capstone · Planning advisor
`far` · 2 milestones, 3.5 h · main-path parts needed run up to step 37. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| AD-C1 Compare and advise | 90 min | suggestions, slack and version differences in one report | reference agreement |
| AD-C2 Spend wisely | 120 min | choose work, checkpoints and speed-ups within budgets and check against the exact schedule | brute-force agreement on small inputs |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| suggestions, slack, differences | AD01, AD02, AD03 |
| budget choices | AD04, AD05, AD06 |
| exact schedule | AD07 |
| carried in by those parts' Required chains | C01, C02, C03, C04, D01, D02, D03, S01, S02, S03, S04 |
| stages 01-08 | assumed done |

### AI · Intervals (algorithm)

Calendars, windows and resource timelines.

#### AI01 · Blackout windows
45 min · `next` · main step 41
- **Goal:** `calendar` turns maintenance windows in any order, overlapping, into a clean effective list
- **Required first:** W03 (parse, don't validate)
- **Used again in:** AI02, AY01, AY02
- **Outside FlowForge:** calendars, on-call schedules
- **Test:** covered instants equal a point-by-point reference; output sorted and minimal; boundary rule stated
- **Craft:** range types and `Ord` (std Range)

#### AI02 · When can it start?
60 min · `next` · main step 42
- **Goal:** given blackout windows, report the earliest allowed start for tasks that cannot pause
- **Required first:** AI01 (Blackout windows), S07 (time-based eligibility)
- **Used again in:** AI04, MX01, MX05
- **Outside FlowForge:** delivery and maintenance scheduling
- **Test:** the time is allowed and no earlier one exists
- **Craft:** checked time arithmetic (jiff)

#### AI03 · How busy did it get?
60 min · `next` · branch
- **Goal:** `usage --peak` reports peak resource use and when
- **Required first:** D01 (durable run records)
- **Used again in:** AI04, AI06, MX04
- **Outside FlowForge:** capacity reports, meeting rooms
- **Test:** equals a point-by-point reference; 100k records under a work budget
- **Craft:** sort keys (std)

#### AI04 · Find me a slot
90 min · `next` · branch
- **Goal:** `slot --need r --for d` finds the earliest time with enough free capacity
- **Required first:** AI02 (When can it start?), AI03 (How busy did it get?)
- **Used again in:** AI05
- **Outside FlowForge:** booking systems
- **Test:** the slot is feasible and earliest by reference
- **Craft:** query API design (jiff)

#### AI05 · Reservations
120 min · `far` · branch
- **Goal:** book and cancel resources ahead; conflicts refused with the conflict named
- **Required first:** AI04 (Find me a slot), A02 (long-running operations as resources)
- **Used again in:** no planned part yet
- **Outside FlowForge:** hotel, GPU and room booking
- **Test:** random sequences match a naive reference; never oversubscribed
- **Craft:** invariant-keeping types (std BTreeMap docs)

#### AI06 · What was running then?
90 min · `far` · branch
- **Goal:** many time-window queries over a large history
- **Required first:** AI03 (How busy did it get?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** observability queries
- **Test:** equals a linear reference; work counter well below full scans on selective queries
- **Craft:** borrowing iterators (std)

#### AI capstone · Resource calendar
`far` · 2 milestones, 2.5 h · main-path parts needed run up to step 42. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| AI-C1 Windows and usage | 60 min | calendar, earliest start and peak usage | point-by-point reference |
| AI-C2 Bookable capacity | 90 min | slots, reservations and history queries through the API | random sequences match reference |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| windows and starts | AI01, AI02 |
| usage and slots | AI03, AI04 |
| reservations and history | AI05, AI06 |
| API and run history | A02, D01 |
| carried in by those parts' Required chains | A01, C01, C02, C03, C04, D02, D03, S07 |
| stages 01-08 | assumed done |

### AY · Greedy (algorithm)

Scheduling and packing decisions that must be justified.

#### AY01 · The most bookings
45 min · `next` · main step 43
- **Goal:** from fixed-time requests on one exclusive resource, accept as many as possible
- **Required first:** AI01 (Blackout windows)
- **Used again in:** no planned part yet
- **Outside FlowForge:** room booking
- **Test:** no overlaps; count equals brute force on small inputs
- **Craft:** stable sorting and tie rules (std)

#### AY02 · How many workers is enough?
60 min · `next` · branch
- **Goal:** for a fixed timetable give the fewest workers and an assignment
- **Required first:** AI01 (Blackout windows)
- **Used again in:** no planned part yet
- **Outside FlowForge:** staffing, platform allocation
- **Test:** assignment conflict-free; no smaller number works (exhaustive on small inputs)
- **Craft:** assignments by index (std)

#### AY03 · Hit the deadlines
60 min · `next` · branch
- **Goal:** order independent tasks with deadlines on one worker so the worst lateness is as small as possible
- **Required first:** S03 (scheduling policy)
- **Used again in:** AY05, MX03
- **Outside FlowForge:** personal and CPU scheduling
- **Test:** worst lateness equals the best over all orders on small inputs
- **Craft:** comparator design (std sort_by_key)

#### AY04 · The cheapest way to combine
60 min · `far` · branch
- **Goal:** many log files are combined pairwise; `compact --plan` minimises total bytes rewritten
- **Required first:** D01 (durable run records)
- **Used again in:** no planned part yet
- **Outside FlowForge:** storage compaction, merge planning
- **Test:** plan cost equals exhaustive search on small inputs
- **Craft:** ordering for plans (std Ord)

#### AY05 · When the obvious rule loses
90 min · `far` · branch
- **Goal:** `hunt` finds small workflows where a simple policy is worse than the exact best, and shrinks them
- **Required first:** AY03 (Hit the deadlines), AD07 (The true best, when small)
- **Used again in:** no planned part yet
- **Outside FlowForge:** reviewing any heuristic
- **Test:** each counterexample is verified by the exact mode and replays from its seed
- **Craft:** strategy traits (proptest)

#### AY capstone · Justified quick decisions
`far` · 2 milestones, 2.5 h · main-path parts needed run up to step 43. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| AY-C1 Decision pack | 60 min | booking, staffing, deadline ordering and combining in one report, each with a written argument for why it is right | exhaustive agreement on small inputs |
| AY-C2 Attack your own rules | 90 min | hunt for inputs where a simple policy loses and report them | verified counterexamples |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| bookings and staffing | AY01, AY02 |
| deadlines and combining | AY03, AY04 |
| counterexample hunting | AY05 |
| calendar inputs | AI01 |
| carried in by those parts' Required chains | AD04, AD07, C01, C02, C03, C04, D01, S01, S02, S03, S04 |
| stages 01-08 | assumed done |

### AB · Bit manipulation (algorithm)

Compact sets: capabilities, permissions, statuses, large batches of questions.

#### AB01 · Which workers can take it?
45 min · `next` · main step 39
- **Goal:** tasks require capabilities and workers offer them; matching explains what is missing, fast at large scale
- **Required first:** S05 (multi-resource admission)
- **Used again in:** AB05, MX04
- **Outside FlowForge:** feature flags, hardware matching
- **Test:** equals reference sets; a stated limit on distinct capabilities is enforced or lifted
- **Craft:** flag types (bitflags)

#### AB02 · Who may do what, combined
45 min · `far` · branch
- **Goal:** permission sets per caller combine from roles
- **Required first:** Z06 (authorization)
- **Used again in:** no planned part yet
- **Outside FlowForge:** permission systems
- **Test:** truth table over combinations; unknown flags rejected on load
- **Craft:** type-safe flags with serde (bitflags)

#### AB03 · A million yes-or-no questions
90 min · `next` · branch
- **Goal:** answer a very large batch of "does a need b" queries on a mid-sized workflow within a memory budget
- **Required first:** AG01 (What does this change touch?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** reachability indexes, access checks
- **Test:** equals the oracle; work and memory counters under budget
- **Craft:** index newtypes (fixedbitset)

#### AB04 · A whole run in a few bytes
60 min · `far` · branch
- **Goal:** status of very large runs is stored and diffed within a size budget
- **Required first:** D01 (durable run records), W09 (task outcome states)
- **Used again in:** no planned part yet
- **Outside FlowForge:** game state, telemetry encoding
- **Test:** exact round trip; size under budget; diffs equal naive comparison
- **Craft:** `repr` and conversions (std TryFrom)

#### AB05 · A few more tasks
90 min · `far` · branch
- **Goal:** the exact scheduler handles somewhat larger inputs within a work budget
- **Required first:** AB01 (Which workers can take it?), AD07 (The true best, when small)
- **Used again in:** MX06
- **Outside FlowForge:** exact solvers
- **Test:** identical answers to AD07; state counts reported and under budget
- **Craft:** compact keys (rustc-hash)

#### AB capstone · Compact cluster state
`far` · 2 milestones, 2.5 h · main-path parts needed run up to step 39. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| AB-C1 Capabilities and permissions | 60 min | matching and permissions for many workers and users | truth-table agreement |
| AB-C2 Large and small | 90 min | huge query batches and compact run status within budgets | size and work budgets |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| matching and permissions | AB01, AB02 |
| batch queries and compact status | AB03, AB04 |
| larger exact solving | AB05 |
| carried in by those parts' Required chains | A01, A02, A04, AD04, AD07, AG01, C01, C02, C03, C04, D01, D02, D03, S01, S02, S03, S04, S05, W09, Z05, Z06 |
| stages 01-08 | assumed done |

### MX · Combinations (combination)

Features that need several things already practised. No new concept is introduced.

#### MX01 · Dry run the whole plan
120 min · `next` · branch
- **Adds:** no new concept; combines the required parts
- **Goal:** `simulate --workers N` predicts start and finish times with priorities and blackout windows
- **Required first:** S04 (heuristic scheduling), AI02 (When can it start?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** terraform plan, deployment previews
- **Test:** prediction equals the virtual-time executor on random inputs
- **Craft:** one core for two modes (cargo, by name)

#### MX02 · Rerun only what matters
90 min · `next` · branch
- **Adds:** no new concept; combines the required parts
- **Goal:** after an edit, list exactly what must rerun and why
- **Required first:** M04 (result caching), AG01 (What does this change touch?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** build systems, data backfills
- **Test:** equals actual cache behaviour on random edits
- **Craft:** explanation types (cargo fingerprint)

#### MX03 · Make the deadline
120 min · `far` · branch
- **Adds:** no new concept; combines the required parts
- **Goal:** choose optional work and an order meeting deadlines with most value; exact when small, gap reported when large
- **Required first:** AD04 (The best use of a budget), AY03 (Hit the deadlines)
- **Used again in:** no planned part yet
- **Outside FlowForge:** sprint and delivery planning
- **Test:** constraints always hold; exact equals brute force; the quick mode never beats exact
- **Craft:** pluggable solver traits (std)

#### MX04 · The cheapest fleet
120 min · `far` · branch
- **Adds:** no new concept; combines the required parts
- **Goal:** from a timetable and priced worker types with capabilities, propose a fleet
- **Required first:** AB01 (Which workers can take it?), AI03 (How busy did it get?) · **Helpful:** AR06 (Make it fit)
- **Used again in:** no planned part yet
- **Outside FlowForge:** cloud cost planning
- **Test:** every task covered; cost equals exhaustive search on small inputs
- **Craft:** search budgets (std)

#### MX05 · If this slips
90 min · `far` · branch
- **Adds:** no new concept; combines the required parts
- **Goal:** `what-if <task> +d` reports which tasks and windows are missed, and why
- **Required first:** AD02 (How much slack?), AI02 (When can it start?) · **Helpful:** AG01 (What does this change touch?)
- **Used again in:** no planned part yet
- **Outside FlowForge:** supply chain and release risk
- **Test:** equals a full re-simulation
- **Craft:** what-if views, `Cow` (std)

#### MX06 · Exact or quick
120 min · `far` · branch
- **Adds:** no new concept; combines the required parts
- **Goal:** return the best found and a proven bound that improve with budget
- **Required first:** AB05 (A few more tasks), AR06 (Make it fit)
- **Used again in:** no planned part yet
- **Outside FlowForge:** solvers, query planners
- **Test:** the bound is never wrong; quality never worsens with more budget
- **Craft:** anytime APIs as iterators (std)

#### MX capstone · The what-if planner
`far` · 3 milestones, 5.5 h · main-path parts needed run up to step 42. Adds no new concept.

| Milestone | Time | Ends in this behaviour | Tested by |
|---|---|---|---|
| MX-C1 Predict | 120 min | simulate a plan with windows, priorities and what-if delays | equals virtual-time execution |
| MX-C2 Optimise | 120 min | deadline, fleet and budgeted schedule choices with explanations and bounds | constraints hold; exact agreement on small inputs |
| MX-C3 Replan after change | 90 min | rerun set and revised prediction after an edit | equals actual execution |

**Readiness**

| The capstone needs | Taught in |
|---|---|
| dry run and slip | MX01, MX05 |
| rerun | MX02 |
| deadline, fleet, exact or quick | MX03, MX04, MX06 |
| controlled time | C04 |
| carried in by those parts' Required chains | AB01, AB05, AD02, AD04, AD07, AG01, AI01, AI02, AI03, AR02, AR03, AR04, AR05, AR06, AY03, C01, C02, C03, D01, D02, D03, M04, S01, S02, S03, S04, S05, S07 |
| stages 01-08 | assumed done |

## Advanced series (after the 16 capstones)

Added 2026-11-27 at the owner's request: six planned, none built. These combine several of the anneal capstones.
Start one only when the 16 capstone series are done. They are not FlowForge solutions: FlowForge itself stays the
owner's to build.

**Rules for each.** The capstones do not teach everything. Give each series its own small preparation exercises for
the new topics before asking you to combine them. Every milestone also needs failure tests, a performance measurement
and a short note on the design choices. Exercises may overlap in ideas, but none may repeat an earlier assessment.

**Order.** A5 and A6 are course-style performance projects; A6 suits following BusTub, and was suggested before A5. Replicated database first; the broker and the search service build on it. Language tools is a separate
path. The API gateway was suggested first and was not added, so the replicated database's preparation must also teach
real network failures: timeouts, retries, half-open connections and overload.

| # | Series | Builds on | Main new topics | Ideas shared with this file |
|---|---|---|---|---|
| A1 | Replicated key-value database | key-value store, mini Redis, lease fencing, async runtime | Raft (elections, log, snapshots, membership changes), linearizable reads, fault injection and history checking, real network failures | N, K |
| A2 | Distributed message broker (Kafka-like) | event ledger, key-value store, job service, A1 | partitioned and replicated log, segments and retention, consumer groups, rebalancing, delivery guarantees, backpressure | Q, N, J |
| A3 | Distributed search service | search engine, typeahead, web server, A1 | sharding, scatter and gather, merging ranked results, live index updates, tail latency, vector and hybrid search | H, V, N |
| A4 | Language and developer tools | Lox interpreter, regex engine | bytecode VM, garbage collection, type checking, a compiler backend (Cranelift), incremental analysis, a language server | G, P |

**A1 milestones.** One node as a state machine over a log; elections in a deterministic simulator; log replication
and safety checks; persistence and restart; snapshots; membership changes; real sockets and a client; a
linearizability check under injected faults.

**A2 milestones.** A segmented log on one node; offsets and consumers; partitions; replication through A1's ideas;
consumer groups and rebalancing; idempotent producers; retention and compaction; failure drills and throughput.

**A3 milestones.** One searchable shard behind HTTP; many shards and merged ranking; replicas and failover; live
indexing; typeahead across shards; hybrid keyword and vector search; slow-shard handling; relevance and latency
benchmarks.

**A4 milestones.** Bytecode and a VM; garbage collector; resolver and type checker; diagnostics; native code for a
subset; incremental queries; language server; fuzzing the front end against the interpreter.

### A5 · Trading order engine (32 challenges, course-style)

Added 2026-11-27. Planned, not built. One growing codebase, with made-up orders; it is not connected to real trading.
Build a simple correct engine first and keep it as the answer to compare faster versions against. Main subjects:
data structures, memory use, queues without locks, recovery and consistently low delays. It reuses ideas from the
event ledger, rate limiter and async runtime capstones. Each challenge has a goal, behaviour tests, hints that start
hidden, a performance experiment where speed matters, and one design question. Times are unmeasured estimates of
45 to 120 minutes each. Put a checkpoint every 5 to 8 challenges.

| Phase | Challenges |
|---|---|
| Correct matching | 1 define orders · 2 add buy and sell orders · 3 match by price and arrival order · 4 partial fills · 5 cancel · 6 replace · 7 market orders · 8 check that quantities add up |
| Better data structures | 9 measure the baseline · 10 find orders quickly · 11 remove them cheaply · 12 organise price levels · 13 reuse memory · 14 handle stale references safely · 15 compare with the simple engine · 16 profile realistic workloads |
| Recovery and communication | 17 record events · 18 replay them · 19 snapshots · 20 recover a damaged log ending · 21 network messages · 22 repeated requests · 23 publish market updates · 24 slow listeners |
| Consistent speed | 25 a bounded queue · 26 a single-producer queue without locks · 27 check memory ordering · 28 reduce cache contention · 29 no allocation while matching · 30 sudden bursts · 31 measure the slowest requests · 32 defend every speed change in a final report |

### A6 · Fast analytics engine (36 challenges, course-style)

Added 2026-11-27. Planned, not built. A follow-on to the BusTub course, built as one growing codebase. It does not
rebuild BusTub's parser, transactions, recovery or locking. Build the row-at-a-time version first and keep it as the
answer to compare against. The same challenge format and time estimates as A5 apply, with a checkpoint every 5 to 8
challenges. When this was added, BusTub's module names and a text search showed no module on column storage or batch
processing; its stages were not all read, so check them for overlap before writing challenges.

| Phase | Challenges |
|---|---|
| Starting point | 1 define a table · 2 repeatable test data · 3 process one row at a time · 4 check answers against a trusted database · 5 measure speed and memory · 6 find the first bottleneck |
| Column storage | 7 columns in memory · 8 missing values · 9 text · 10 a file format · 11 read only needed columns · 12 reject damaged or hostile files |
| Compression | 13 repeated-value encoding · 14 dictionaries for text · 15 pack small numbers · 16 store differences · 17 filter compressed data · 18 choose an encoding from measurements |
| Faster queries | 19 process batches · 20 filter batches · 21 skip unwanted rows · 22 totals · 23 grouping · 24 skip blocks using minimum and maximum |
| CPU performance | 25 remove repeated allocation · 26 data layout · 27 fewer unpredictable branches · 28 CPU instructions on several values · 29 use several cores · 30 reduce thread contention |
| Large workloads | 31 joins · 32 uneven data · 33 sort and top results · 34 spill to disk · 35 load columns late · 36 benchmark against the first version and an established database |

## Database design foundations (planned series, 60 exercises)

Added 2026-11-27 at the owner's request. Planned, not built. The foundations for designing a database system in
general. It is not tied to BusTub: it covers other designs too, and also prepares you for that course, which stays a
course. Do the groups in order, or take a group just before the work that needs it. Skip an exercise you can already do.

**Rules.**
- Each exercise uses its own small setting, so it teaches the idea without giving away a BusTub or capstone solution.
- No exercise repeats a BusTub stage, challenge or an existing problem. Reference an existing problem when it already
  teaches the idea, especially from the key-value store, mini Redis and event ledger series. This list has not yet
  been compared with the app's existing problems or with every BusTub stage (only module descriptions and some stage
  titles were read), so do that before writing any exercise.
- 45 to 120 minutes each (unmeasured), with a goal, behaviour tests, hidden hints and one design question.
- Each group ends with a short checkpoint that mixes its ideas.

| Group (BusTub modules it also helps with) | Exercises |
|---|---|
| 1 · Bytes and files (00, 01) | 1 pack a game save into fixed-size bytes · 2 edit one record in a file without rewriting it · 3 reuse the gaps left by deleted records · 4 reject a damaged or cut-off file · 5 checkpoint: a fixed-slot address book |
| 2 · Background work (02) | 6 hand a result back to the thread that asked · 7 a worker that finishes accepted jobs before stopping · 8 let many readers or one writer into a document · 9 keep jobs for one customer in order while others run |
| 3 · Choosing what to keep (03–05) | 10 remove any item from a linked list stored in a vector · 11 spot a handle to a reused slot · 12 a small recently-used list for open files · 13 keep each key's last few visit times · 14 find the best item to remove when priorities change · 15 remember recently removed keys and learn from their return |
| 4 · Pages in use (06–08) | 16 a library desk where borrowed items cannot be thrown away · 17 save changed items before reusing their space · 18 give a resource back automatically, also on panic · 19 lock two items without deadlock · 20 read numbers and text safely from raw bytes · 21 keep entries sorted inside a fixed-size byte block |
| 5 · Indexes (09–11) | 22 a phone book whose buckets split as it grows · 23 check a hash structure against a simple map with random operations · 24 split and join full pages of a paged list · 25 check a tree's rules after every change · 26 hand locks from parent to child while walking a tree · 27 delete by marking, then clean up safely |
| 6 · Rows and expressions (12–15b) | 28 a spreadsheet value that can be missing · 29 arithmetic that reports overflow · 30 true, false and unknown in a form validator · 31 lay out fixed and variable fields in one record · 32 evaluate a small filter expression tree · 33 combine errors from layers while keeping their causes |
| 7 · Query execution (16–19, 28) | 34 chain steps that produce one item at a time · 35 total sales per group without storing every row · 36 match two lists three different ways and compare cost · 37 sort data larger than the memory allowed · 38 keep only the top results · 39 rewrite a plan and prove the answers are unchanged |
| 8 · Transactions and recovery (20–23) | 40 a document that readers can view as of an earlier version · 41 detect two edits based on the same old version · 42 recover a counter service after a stop at any step · 43 find a cycle of people waiting for each other · 44 a lock queue where nobody waits forever |
| 9 · Write-optimised storage (none) | 45 collect writes in a sorted memory table · 46 write a sorted immutable file with an index · 47 read across several files with newest and deleted values · 48 merge files without losing or reviving data · 49 skip files that cannot hold a key · 50 measure read, write and space cost of two merge policies |
| 10 · Estimating and choosing plans (19) | 51 collect statistics about a column · 52 estimate how many rows a filter keeps, then check the error · 53 choose between scanning and an index from a cost model · 54 choose a join order for a few tables · 55 find where a wrong estimate chose a bad plan |
| 11 · Design decisions (none) | 56 model a booking system's tables, keys and rules · 57 remove duplicated facts, then measure what copying them back buys · 58 choose indexes for a given workload and write cost · 59 compare row, column and write-optimised layouts on three workloads · 60 final design: justify storage, indexes, transactions and recovery for one product, with the failures it accepts |

**BusTub modules 24–27** (persistent trie, skip list, Robin Hood hashing, sketches) have no group: each is already a
self-contained exercise.

**Left out on purpose.** Data spread across several machines belongs to advanced series A1; deep column storage and
CPU speed belong to A6.

## What `./ff` would need to show series (proposal; `ff` is unchanged)

How it works today (from reading the script):

- the stage count is the number of `tests/stage_*.rs` files, and it loops `1..count`, so numbers must be **contiguous**;
  a gap prints empty names and a stage numbered above the count is never listed;
- stage `i` is found by the two-digit patterns `tests/stage_NN_*.rs` and `tasks/NN-*.md` (first match wins);
- `list` runs `cargo test` once per stage to decide `[x]`; `next` prints the first failing stage in **numeric** order;
- `hint N L` prints the `### Hint L` block, stopping at the next `## ` heading other than Hints;
- `test N` pipes cargo through `grep -v`, so the command's exit status is grep's, not the tests'.

Proposed:

1. A mentor-owned manifest, for example `series.tsv`: part id, stage number or `-`, main step or `-`, minutes,
   horizon, required ids, title. `ff` reads it; series data stays out of task files, so reused-knowledge links never
   appear in task statements.
2. `./ff series` (each series with progress), `./ff series C` (its parts), `./ff path` (main path with the next step
   marked), `./ff show C01` (accept part ids as well as numbers).
3. `next` = first unfinished main-path part that has a stage file; if the next part has none, say "not written yet".
   Print, separately, branches whose first part is open.
4. Locked state for parts with unfinished Required parts, as advice only.
5. Cache pass results, or run `cargo test` once and parse per-binary results; per-stage runs will not scale.
6. Never print the appendices.

**Numbering decision for you.** Because of the contiguity rule, stage numbers should be allocated when a stage is
written, with this file's ids as the stable names. On this path the next files would be 09 sequential run (C01), then
the trace checker only if you write it, then the concurrent parts, and critical path much earlier than `ROADMAP.md`'s
13. `ROADMAP.md` numbers (and the "stage 15" wording in stages 02-03) then become labels that no longer match. The
alternative, teaching `ff` to accept gaps and reading order from the manifest, keeps the old numbers.
Also decide whether the concurrent executor stays one stage (C02 and C03, 120 min together) or two; `ROADMAP.md` also
puts a per-task timeout into stage 09, which this plan moves to W12.

## What to write first

1. **C01 Sequential run** (stage 09). It fixes the run trace that most later tests depend on.
2. **C02 and C03, concurrent run with limit N** (the ladder's concurrent executor; one or two stage files, see above),
   including the hold-open task kind. TR01 can ship beside C01 as a companion.
3. **S01 Critical path.** It needs only stage 08, completes the ladder's third item, and gives a result later
   scheduling parts compare against. Its stage page must follow the spoiler findings below.

## Appendix A: links that give something away (spoilers)

Open an entry only after finishing the later part. These are optional knowledge links, never requirements, and they
must not be copied into task statements.

| Earlier | Later | The link |
|---|---|---|
| W06 A safe order | AB03 A million yes-or-no questions | ordering again, combined with compact sets |
| AD07 The true best, when small | AB05 A few more tasks | compact set encodings change what size the exact solver reaches |
| S01 Critical path | AD02 How much slack? | extends the same analysis in the other direction |
| AD01 Did you mean? | AD03 What changed between versions? | the same family: comparing two sequences |
| S01 Critical path | AD05 Where to save progress | the same family of reasoning along a chain |
| AD04 The best use of a budget | AD06 Spend the speed-up budget | the same family, moved onto a tree |
| AR05 Pick a legal set | AD07 The true best, when small | compare exhaustive search with reusing sub-answers |
| W05 No cycles | AG05 Who is really in the loop? | extends the stage 5 invariant from yes/no to which |
| W07 Same answer every time | AG06 Links that say nothing new | the same lean on ordering |
| AB03 A million yes-or-no questions | AG06 Links that say nothing new | large batches of "does a need b" are what this lint asks internally |
| S11 Optimal assignment | AG09 The widest moment | a matching-style technique answers the width question |
| AI01 Blackout windows | AI04 Find me a slot | normalised windows come back inside slot search |
| AI03 How busy did it get? | AY02 How many workers is enough? | the peak question and the fewest-workers question are related |
| AR06 Make it fit | MX06 Exact or quick | pruned search is one half of an anytime solver |
| W06 A safe order | S01 Critical path | the order from stage 6 is something this analysis can lean on |
| AG04 How are these two related? | V08 HNSW | layered search over a graph returns inside an index |

**Algorithm concept rows** (families only): nested and exhaustive search: AR01 first, again AR04, AR05, AR06, MX06.
Traversing dependency structure: stage 05, again AG01, AG02, AG04, AG05, J02. Reusing answers to smaller questions:
AD01 first, again AD02-AD07, MX03. Time ranges: AI01 first, again AI02-AI06, MX01, MX05. Justifying a local choice:
AY01 first, again AY02-AY04, and challenged in AY05. Compact sets: AB01 first, again AB02-AB05, H03, K07. Compare
each family with the others in the MX parts.

## Appendix B: cookbook pointers for discovery parts (spoilers)

Open only after your own first pass. File names alone can reveal a method. Title lines were read; bodies mostly
were not.

| Part or series | Files under `cookbook/examples/` |
|---|---|
| W05 No cycles | `algorithms/index_graph`, `memory/arena_ids` |
| W06 A safe order | `algorithms/index_graph` |
| W07 Same answer every time | `data_structures/heap_tiebreak`, `dev_tools/shrinking` |
| AR | `algorithms/backtracking_branch_bound` |
| AG | `algorithms/index_graph`, `algorithms/tarjan_scc` (see Appendix C), `algorithms/union_find`, `algorithms/mst_bridges`, `algorithms/csr_graph` |
| AD | `algorithms/dp_zoo`, `algorithms/dp_edit_distance`, `algorithms/dp_lcs_diff`, `algorithms/dp_knapsack`, `algorithms/dp_memoization` |
| AI, AY | `algorithms/greedy_and_interval_algorithms` |
| AB | `algorithms/bit_tricks`, `algorithms/bit_manipulation_real_world` |

## Appendix C: spoiler findings in existing material

Found by keyword search over `cookbook/examples` (dependency, ordering, indegree, cycle and critical-path terms) and
by opening the matching sections. It was not a read of all the files, so there may be more. Nothing was edited. This
appendix names methods.

**Gives away or nearly gives away a stage**

| File | What it hands over | Stages |
|---|---|---|
| `database/transaction_theory_serializability` | a short working Kahn topological order with smallest-ready tie-break, returning none on a cycle; no warning | 05, 06, 07 (most serious) |
| `database/timestamp_ordering_occ` | a working indegree-based acyclicity check; no warning | 05 |
| `interview/concurrency_interview_questions` | Kahn-based cycle check in Q11; has a SPOILER line | 05 |
| `memory/garbage_collectors` | indegree peeling that counts what cannot be removed; no warning seen in header | 05 |
| `algorithms/tarjan_scc` | finds cycle members with an iterative component algorithm; **stage 05's Cookbook section links it**, and its header mentions stage 8 | 05, 08, AG05 |
| `algorithms/dp_edit_distance` | edit distance with name suggestions; **stage 04's Cookbook section links it** | AD01 (not stage 04 itself) |
| `algorithms/dp_count_paths` | explains the predecessor-first recurrence shape on a DAG and invites swapping the combine step for longest or earliest | critical path (S01), AD02 |

**Near, judge before linking:** `distributed/clock_sync_and_trace_context` computes a "critical path", but over trace
span trees (a different problem). `compilers_runtimes/dataflow_analysis_framework` and `ssa_register_allocation`
contain graph visit orders. The cookbook README says recipes avoid giving away stage solutions; the first rows
contradict that. `ROADMAP.md` lists critical-path DP as a planned recipe "add with stage 13", which would need care.

**Cookbook files that do a later part's core idea in another domain** (marked `similar` above, or listed in Appendix
B): among others `distributed/token_bucket` and `capstones/rate_limiter_lab` (L01 to L07; the lab contains six
limiters and adaptive limits), `concurrency/joinset_semaphore` and `async/worker_pool` (the concurrent executor;
the former's header says "Use in: concurrent executor stage"), `distributed/retry_backoff_jitter` (W11),
`web/axum_sse_replay` (A08; its sample data uses FlowForge-like task fields), `web/axum_idempotency` (A05).
Do not link these from a stage page before the pass.

**Outside the cookbook:** `flowforge.md` states the Kahn steps, the critical-path formula and a concurrency-limit
sketch. Stage 05's Concepts and "How to approach it" sections outline the method before any hint is requested, stage
06's Background names it, and stage 07's Concepts names the collections; hints are meant to be the place for that.
These files were left unchanged; whether to tighten future pages is your call. Future task pages for S01 and the
algorithm series should name behaviour only above `## Hints`.

## Not verified

- Time boxes: estimates only; no part was attempted or timed.
- Capstone readiness: the script checks that every listed id exists and that each series' capstone reaches all of that
  series' parts. That the listed concepts are complete is my judgement; a gap may only show when a milestone is written.
- No test was run, and `./ff` was only read.
- No external repository, paper or link was opened for this plan; repository names are reading candidates. Whether a
  named repository really shows the craft topic well is unchecked.
- Cookbook: file existence was checked by script. The title line of every referenced file was read, longer header
  comments for many, and parts of a few bodies; relevance rests on that. No example was built or run, and the spoiler
  search was keyword-based.
- Feasibility of far parts, their test designs, platform limits (sandboxing, io_uring) and the proposed CLI surfaces.
- The anneal exercises and other repositories were not opened.
