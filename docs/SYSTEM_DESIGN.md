# System design prep (the fourth area)

The owner asked on 2026-10-31 for system design interview prep: high-level design, low-level design, API design and
behavioral rounds, built to prepare well, without AI. Mockup (approved as "lets build system design"):
https://claude.ai/artifact/VRBAV6ffok6j49qc6jnsCD, plus the ten additions listed under "What it covers".

## The loop

Learn a concept and answer open checks in your own words, practise a problem against the clock step by step, compare
with a written rubric and mark each point got it, partly or missed, review what you missed on a spaced schedule, and
sit mock rounds. Nothing is graded by a model: rubrics are written, the owner marks, FSRS schedules. Low-level design
is checked by tests in the runner.

## What it covers

Phase 1 (first ship): the area, Home, Learn, the problem list, the high-level drill, Compare, Review.

1. **Problem list**: every problem with its kind, minutes, the concepts it uses, companies, attempts and best score.
2. **Key technologies**: when to pick Redis, Kafka, Postgres, Cassandra, DynamoDB, Elasticsearch; what each is bad
   at; its numbers. Concept pages in the `technology` group.
3. **Patterns**: scaling reads, scaling writes, contention, realtime updates, long-running jobs, large blobs,
   multi-step processes. Concept pages in the `pattern` group.
4. **Estimation**: the numbers to know and how to estimate (phase 1 as concepts with checks; timed drills phase 2).
5. **Deep-dive probes**: what an interviewer pushes on, written per problem, shown one at a time in the last step.
6. **Level expectations**: each rubric point is tagged `mid`, `senior` or `staff`.
7. **Reference beside your answer** in Compare, step by step.
8. **Low-level theory**: OOP basics, design principles, patterns, concurrency (phase 2, with the LLD problems).
9. **Behavioral additions**: a 90-second intro, stories mapped to company values, questions to ask (phase 3).
10. **One daily plan** with DSA (phase 3).

Phase 2: API drills, low-level problems with tests, estimation drills, LLD theory. Phase 3: Stories, Mock, the shared
daily plan. Optional later: recording yourself, case studies, a friend-mock view.

## Content (`content/sd/`)

Written for anneal; nothing is copied from another course. Checked by `anneal-content` at load (`anneal validate`).

- `concepts/<id>.toml`: `title`, `group` (`fundamentals`, `storage`, `scale`, `communication`, `technology`,
  `pattern`, `lld`), `minutes`, `lede`, `[[sections]]` (`heading`, `body` in Markdown), `say` (one sentence to say in an
  interview), `[[checks]]` (`prompt`, `answer`), `related` (concept ids).
- `problems/<id>.toml`: `title`, `kind` (`hld` now; `api`, `lld` later), `minutes`, `prompt`, `uses` (concept ids),
  `companies`, `hints`, `[[steps]]` (`name`, `minutes`, `prompt`, `reference`, `[[steps.points]]` with `id`, `text`,
  `level`), `[[probes]]` (`question`, `answer`).

## Data (migration 0022, additive)

- `sd_attempts`: one row per drill: problem, when, seconds, each step's answer and time, the diagram, hints used,
  and the marks (`{point id: got | part | miss}`).
- `sd_cards`: review cards (a missed or partly rubric point, a missed concept check) with FSRS memory, due date,
  reps, lapses, last grade and history. Card id `point:<problem>:<point>` or `check:<concept>:<n>`.

Both are in the backup (`backup.rs` TABLES). Nothing deletes rows.

## API

- `GET /api/sd`: concepts and problems with standing, due cards, readiness per section.
- `GET /api/sd/concepts/{id}`, `GET /api/sd/problems/{id}`.
- `POST /api/sd/attempts`: saves a finished drill; `PUT /api/sd/attempts/{id}/marks`: the marks, which add or
  refresh cards for missed and partly points.
- `POST /api/sd/checks`: a concept check marked; a miss adds a card.
- `GET /api/sd/review`, `POST /api/sd/review/{card}`: the due cards and a grade.

## The catalogue (researched 2026-10-31)

Built from the topics these cover, written in anneal's own words: Hello Interview's public lists (delivery framework, core
concepts, patterns, key technologies, problem breakdowns), the system design primer (github.com/donnemartin/system-design-primer),
Karan Pratap Singh's system design book (github.com/karanpratapsingh/system-design), AlgoMaster's lists
(github.com/ashishps1/awesome-system-design-resources, awesome-low-level-design), ByteByteGo's system-design-101, the tables of
contents of *Designing Data-Intensive Applications* (2nd ed., 2026) and Alex Xu's *System Design Interview* volumes 1 and 2,
the Rust patterns book (rust-unofficial/patterns) and Rust interview guides (Send/Sync, blocking the executor, async locks,
graceful shutdown, backpressure). Facts on each page are checked against official documentation (Kafka, Redis, Postgres,
Cassandra, DynamoDB, Temporal, etcd) and the original papers where there is one.

Groups (`group` in a concept file) and what each holds; written pages are marked ✓.

- **fundamentals**: the delivery framework ✓, numbers to know ✓, estimation ✓, scale from zero to millions, availability
  in nines (sequence vs parallel), latency vs throughput.
- **core**: networking (IP, DNS, TCP/UDP, HTTP/2/3, TLS), load balancing (L4/L7, algorithms, health checks), proxies and
  API gateways, CDN (push/pull), API design ✓ (REST, GraphQL, gRPC, idempotency, pagination, webhooks), data modeling ✓,
  SQL vs NoSQL and database types, indexes ✓, caching ✓ (strategies, eviction, invalidation), sharding ✓, replication ✓,
  consistent hashing ✓, CAP, PACELC and consistency models ✓, storage types (block, file, object), unique ID generation
  (Snowflake, ULID, ranges), authentication and authorisation, observability (logs, metrics, traces).
- **pattern**: realtime updates ✓, contention ✓, scaling reads ✓, scaling writes ✓, queues and streams ✓, long-running
  tasks, large blobs (presigned URLs, multipart, chunking), multi-step processes (sagas, workflows), the outbox pattern,
  event sourcing and CQRS, fan-out on write vs read, backpressure and load shedding, retries with backoff, circuit breakers
  and bulkheads, rate limiting algorithms, leader election, service discovery, microservices vs monolith.
- **technology**: Kafka ✓, Redis ✓, Postgres ✓, Cassandra, DynamoDB, Elasticsearch, Temporal, API gateway, ZooKeeper and
  etcd, Flink, S3 and object storage, RabbitMQ/SQS, MongoDB, ClickHouse and column stores, Spark.
- **advanced**: proximity search (geohash, quadtree, S2, H3), typeahead and tries, vector databases (embeddings, HNSW, IVF),
  time-series databases, change data capture, CRDTs and operational transformation, probabilistic structures (Bloom filter,
  HyperLogLog, count-min sketch, top-k), stream processing (windows, watermarks, exactly-once), search ranking (BM25),
  multi-region and geo-replication, data lakes and batch processing.
- **distributed** (DDIA): clocks and ordering (Lamport, vector clocks, hybrid logical clocks, TrueTime), consensus (Raft,
  Paxos), linearizability, quorums, anti-entropy and hinted handoff, gossip and failure detection, transactions and
  isolation anomalies (dirty read, lost update, write skew, phantoms), distributed transactions (2PC, 3PC, sagas), leases
  and fencing tokens, storage engines (LSM tree vs B-tree, WAL, compaction), encoding and schema evolution, partitioning
  and rebalancing, batch vs stream (Lambda, Kappa).
- **lld**: OOP and class relationships, SOLID, DRY/KISS/YAGNI, design patterns (strategy, observer, factory, builder,
  state, decorator, adapter, command, singleton and why to avoid it), UML (class, sequence, state), concurrency primitives
  (mutex, semaphore, condition variable, CAS, reentrant and read-write locks), deadlock and livelock, concurrency patterns
  (producer-consumer, thread pool, reader-writer), how to answer an LLD round.
- **rust**: idioms (borrowed argument types, `Default`, `mem::take`, RAII), design patterns in Rust (newtype, builder,
  typestate, strategy with traits or closures, command, visitor, RAII guards, extension traits), generics vs trait
  objects, error handling (`thiserror`, `anyhow`, `?`), interior mutability (`Cell`, `RefCell`, `Mutex`, `RwLock`,
  atomics), `Send` and `Sync`, async architecture (Tokio runtime, tasks, `spawn_blocking`, async locks, channels,
  cancellation and graceful shutdown, backpressure, fan-out/fan-in, actors), anti-patterns (clone to please the borrow
  checker, `Deref` polymorphism), Rust system design questions (a thread pool, a concurrent map, a rate limiter, a
  blocking queue, an async cache with TTL, a log-structured store, zero-copy parsing).

Problems (`kind`): **hld**: URL shortener ✓, rate limiter ✓, news feed ✓, Uber ✓, Dropbox / Google Drive ✓, YouTube / Netflix ✓,
Google Docs ✓, WhatsApp / chat ✓, Ticketmaster ✓, typeahead ✓, web crawler, notification system, payment system, digital wallet,
Yelp / proximity, nearby friends, Google Maps, Instagram, top-k / leaderboard, ad click aggregator, distributed cache,
key-value store, distributed message queue, job scheduler, metrics and monitoring, stock exchange / Robinhood, online
auction, LeetCode (code judge), Strava, Tinder, Google News, flash sale, hotel reservation, email service, S3-like object
storage, live comments, LLM chat serving, unique ID generator, search engine. **api**: pagination, idempotent payments,
rate limits and errors, versioning, long-running jobs, file upload, webhooks, search filters. **lld**: parking lot,
elevator, LRU cache, rate limiter, logger, vending machine, pub/sub, movie ticket booking with seat locks, Splitwise,
chess, a thread-safe cache with TTL, a concurrent hash map, a blocking queue.

Typeahead is a problem with an advanced page on tries behind it; CRDTs are an advanced page linked from Google Docs.

### Page components

Markdown with callouts (`> [!NOTE]`, `> [!TIP]`, `> [!WARNING]`, `> [!INTERVIEW]` for what to say, `> [!ASIDE]`), tables,
code (Rust, Python, SQL, HTTP), and animated figures (`[[figures]]`, drawn by the web app). Every page ends with open
checks; problems carry a rubric tagged by level and probes for the deep dives.

## AI help (owner asked 2026-10-31)

Off unless the server's environment sets it (`crates/api/src/ai.rs`): `ANNEAL_AI_PROVIDER` (`gemini`, or `openai` for any
OpenAI-compatible server such as Ollama on a Mac), `ANNEAL_AI_KEY`, `ANNEAL_AI_MODEL` (default `gemini-3.5-flash`),
`ANNEAL_AI_URL` (for `openai`, default `http://localhost:11434/v1`). On production the key goes in `deploy/.env` on the
server only; `deploy/compose.prod.yml` passes it through and leaves it empty otherwise. What it does: "Ask about this page"
on concept pages, the DSA problem page and a drill's Compare; and in Compare, an interviewer that marks the attempt against
the rubric (a suggested mark and reason per point, two or three follow-ups, a summary), kept in `sd_attempts.ai_review`
(migration 0023). The owner's marks stay theirs; "Use its marks where I have not marked" only fills gaps.
