# Learning paths

Three ways in. Pick by your situation:

1. **I want to learn a subject from zero** -> a *track* (ordered curriculum, with a goal per step).
2. **I am building something right now** -> a *building guide* (read at the moment you hit that sub-problem).
3. **I am looking at one folder** -> its *folder guide* (teaching order inside the chapter, generated from recipe headers).

Recipes are never moved, so names and links elsewhere stay valid. 🔜 marks a recipe that is planned but not written yet.

## Tracks

- [Rust foundations: from syntax to idiomatic code](tracks/01_rust_foundations.md) (26 recipes)
- [Memory, layout and unsafe](tracks/02_memory_unsafe.md) (17 recipes)
- [Concurrency and async](tracks/03_concurrency_async.md) (27 recipes)
- [Data structures and algorithms in Rust](tracks/04_data_structures_algorithms.md) (30 recipes)
- [Performance engineering](tracks/05_performance.md) (14 recipes)
- [Databases: a CMU 15-445 -> 15-721 -> 15-799 path](tracks/06_databases_cmu.md) (48 recipes)
- [Distributed systems](tracks/07_distributed_systems.md) (20 recipes)
- [Compilers and languages](tracks/08_compilers_languages.md) (14 recipes)
- [Networking and web services](tracks/09_networking_web.md) (20 recipes)
- [Search and vector similarity](tracks/10_search_vectors.md) (12 recipes)
- [Operating-system and storage internals](tracks/11_os_storage_systems.md) (15 recipes)

## Building guides

- [A workflow / job orchestration engine (FlowForge)](building/workflow_engine.md) (29 recipes)
- [A production-style REST service](building/rest_service.md) (20 recipes)
- [A database / storage engine](building/database_engine.md) (18 recipes)
- [A search engine or autocomplete](building/search_engine.md) (12 recipes)
- [A vector database](building/vector_search.md) (11 recipes)
- [A message queue / Kafka-like log](building/message_queue.md) (10 recipes)
- [A cache / Redis-like server](building/cache_or_kv_server.md) (13 recipes)
- [A language, DSL or compiler](building/language_or_dsl.md) (12 recipes)
- [Leader election, locks and coordination](building/distributed_lock_leader.md) (6 recipes)
- [A command-line tool](building/cli_tool.md) (11 recipes)
- [A high-performance async network server](building/async_runtime_server.md) (13 recipes)

## Folder guides

- [`algorithms`](folders/algorithms.md) (27 recipes)
- [`async`](folders/async.md) (10 recipes)
- [`capstones`](folders/capstones.md) (16 recipes)
- [`compilers_runtimes`](folders/compilers_runtimes.md) (4 recipes)
- [`concurrency`](folders/concurrency.md) (20 recipes)
- [`cryptography`](folders/cryptography.md) (8 recipes)
- [`data_structures`](folders/data_structures.md) (27 recipes)
- [`database`](folders/database.md) (36 recipes)
- [`dev_tools`](folders/dev_tools.md) (4 recipes)
- [`distributed`](folders/distributed.md) (19 recipes)
- [`encoding_compression`](folders/encoding_compression.md) (4 recipes)
- [`errors`](folders/errors.md) (5 recipes)
- [`graphics`](folders/graphics.md) (6 recipes)
- [`interview`](folders/interview.md) (15 recipes)
- [`language`](folders/language.md) (33 recipes)
- [`math_science`](folders/math_science.md) (7 recipes)
- [`memory`](folders/memory.md) (5 recipes)
- [`networking`](folders/networking.md) (11 recipes)
- [`os_and_storage`](folders/os_and_storage.md) (14 recipes)
- [`patterns`](folders/patterns.md) (11 recipes)
- [`perf`](folders/perf.md) (15 recipes)
- [`search_vectors`](folders/search_vectors.md) (8 recipes)
- [`text_and_parsing`](folders/text_and_parsing.md) (10 recipes)
- [`unsafe_ffi`](folders/unsafe_ffi.md) (7 recipes)
- [`web`](folders/web.md) (17 recipes)

## Suggested overall order

`01_rust_foundations` -> `04_data_structures_algorithms` (alongside FlowForge stages 1-8) -> `03_concurrency_async` -> `05_performance` -> then branch by interest: databases `06`, distributed `07`, compilers `08`, web `09`, search/vectors `10`, systems `11`. `02_memory_unsafe` only after foundations and some concurrency.
