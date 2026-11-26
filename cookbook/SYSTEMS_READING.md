# Systems Reading and Watching List

Every link below returned HTTP 200 when checked. Cross-references to cookbook recipes are omitted on purpose.

## Rust

- [The Rust Programming Language](https://rust-lang.github.io/book/): the official baseline; ownership, traits, error handling, modules.
- [The Rustonomicon](https://doc.rust-lang.org/nomicon/): how `unsafe` really works: aliasing, layout, drop check, variance.
- [Rust Atomics and Locks](https://marabos.nl/atomics/): memory ordering, building a mutex, channel and Arc from scratch.
- [Rust for Rustaceans](https://rust-for-rustaceans.com/): API design, testing, async internals and unsafe at an intermediate-to-advanced level.
- [Effective Rust](https://www.lurklurk.org/effective-rust/): 35 items of practical guidance on types, traits and tooling.
- [Comprehensive Rust](https://google.github.io/comprehensive-rust/): Google's course; fast tour including concurrency and unsafe.
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/): build settings, profiling, allocation and data-structure tuning.
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/): naming, interoperability and stability checklists for public APIs.

## OS and computer architecture

- [Operating Systems: Three Easy Pieces](https://pages.cs.wisc.edu/~remzi/OSTEP/): free book on virtualisation, concurrency and persistence.
- [What Every Programmer Should Know About Memory (Drepper)](https://people.freebsd.org/~lstewart/articles/cpumemory.pdf): caches, TLBs, NUMA and why access patterns dominate performance.
- [Computer Systems: A Programmer's Perspective](https://csapp.cs.cmu.edu/): the book's site; machine code, linking, virtual memory, I/O and networking from the programmer's side.
- [Algorithms for Modern Hardware](https://en.algorithmica.org/hpc/): SIMD, cache-aware data structures, branch prediction and measured micro-optimisation.

## Databases

- [CMU 15-445 Database Systems](https://15445.courses.cs.cmu.edu/): storage, buffer pools, indexes, query execution, concurrency control and recovery.
- [CMU 15-721 Advanced Database Systems](https://15721.courses.cs.cmu.edu/): modern in-memory, columnar and vectorised engines.
- [Database Internals (Alex Petrov)](https://www.databass.dev/): B-trees, LSM trees, and distributed database building blocks.
- [Architecture of a Database System](https://dsf.berkeley.edu/papers/fntdb07-architecture.pdf): the canonical map of a relational engine's components.
- [mini-lsm](https://github.com/skyzh/mini-lsm): build an LSM storage engine in Rust step by step.
- [Readings in Database Systems (Red Book)](http://www.redbook.io/): curated papers with commentary on data-system design.

## Distributed systems

- [Designing Data-Intensive Applications](https://dataintensive.net/): replication, partitioning, transactions and consensus trade-offs.
- [MIT 6.5840 (formerly 6.824)](https://pdos.csail.mit.edu/6.824/): lectures, papers and labs, including building Raft and a sharded KV store.
- [Raft](https://raft.github.io/): visualisation and resources; the paper is [In Search of an Understandable Consensus Algorithm](https://raft.github.io/raft.pdf).
- [Dynamo paper](https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf): eventual consistency, consistent hashing, vector clocks, sloppy quorums.
- [Spanner paper](https://research.google/pubs/spanner-googles-globally-distributed-database-2/): externally consistent transactions with TrueTime.
- [Bigtable paper](https://research.google/pubs/bigtable-a-distributed-storage-system-for-structured-data/): the wide-column and SSTable design behind many stores.
- [Jepsen](https://jepsen.io/): analyses of how real databases fail under partitions; learn to think adversarially about consistency claims.
- [Fly.io Distributed Systems Challenges](https://fly.io/dist-sys/): hands-on Maelstrom exercises (broadcast, CRDTs, Kafka-style log).
- [Martin Kleppmann's distributed systems lectures](https://www.youtube.com/playlist?list=PLeKd45zvjcDFUEv_ohr_HdUFe97RItdiB): a compact university course on time, broadcast, replication and consensus.

## Concurrency and memory models

- [Preshing on Programming](https://preshing.com/): clear essays on memory ordering, lock-free programming and atomics.
- [The Art of Multiprocessor Programming (Herlihy and Shavit)](https://www.elsevier.com/books/the-art-of-multiprocessor-programming/herlihy/978-0-12-415950-1): the textbook for linearizability, progress guarantees and lock-free structures.
- [Linux kernel memory barriers](https://www.kernel.org/doc/Documentation/memory-barriers.txt): hardware reordering explained from first principles.
- [Linux kernel memory model docs](https://www.kernel.org/doc/html/latest/dev-tools/lkmm/index.html): a formal, tool-checkable model and litmus tests.
- [std::memory_order on cppreference](https://en.cppreference.com/w/cpp/atomic/memory_order): the ordering semantics Rust's atomics inherit (Relaxed, Acquire, Release, AcqRel, SeqCst).

## Networking

- [Beej's Guide to Network Programming](https://beej.us/guide/bgnet/): sockets API in practice; the base for understanding mio and tokio.
- [High Performance Browser Networking](https://hpbn.co/): TCP, TLS, HTTP/2 and latency behaviour that shapes server design.

## Compilers and languages

- [Crafting Interpreters](https://craftinginterpreters.com/): build a tree-walking interpreter and a bytecode VM.
- [matklad's blog](https://matklad.github.io/): rust-analyzer author on architecture, testing, build systems and Rust style.
- [Cranelift](https://cranelift.dev/): the code generator used by wasmtime; a compact, readable backend.
- [Wasmtime documentation](https://docs.wasmtime.dev/): embedding a WebAssembly runtime and its sandboxing model.

## Security and cryptography

- [Cryptopals](https://cryptopals.com/): learn real attacks by implementing them.
- [RustCrypto](https://github.com/RustCrypto): idiomatic pure-Rust primitives; read trait design for hashes, AEADs and signatures.

## Algorithms

- [cp-algorithms](https://cp-algorithms.com/): concise write-ups of graphs, strings, number theory and data structures.
- [Competitive Programmer's Handbook](https://cses.fi/book/book.pdf): free book on practical algorithm techniques.
- CLRS (Introduction to Algorithms, MIT Press): the standard reference textbook; no verified link, so search the title.

## Read real code

- [ripgrep](https://github.com/BurntSushi/ripgrep): parallel directory walking, buffer management and fast search.
- [regex](https://github.com/rust-lang/regex): automata engines, literal optimisations, and strict API design.
- [tokio](https://github.com/tokio-rs/tokio): scheduler, timers, I/O driver and sync primitives.
- [mio](https://github.com/tokio-rs/mio): thin cross-platform epoll/kqueue/IOCP abstraction.
- [hyper](https://github.com/hyperium/hyper): HTTP protocol state machines and zero-copy buffer handling.
- [tantivy](https://github.com/quickwit-oss/tantivy): inverted indexes, segments, and query execution.
- [rust-analyzer](https://github.com/rust-lang/rust-analyzer): incremental computation and large-scale architecture (see its architecture docs).
- [sled](https://github.com/spacejam/sled): an embedded database experiment; lock-free structures and their pitfalls.
- [redb](https://github.com/cberner/redb): a copy-on-write B-tree embedded database with simple, readable design.
- [TiKV](https://github.com/tikv/tikv): Raft-replicated distributed key-value store built on RocksDB.
- [DataFusion](https://github.com/apache/datafusion): a query engine over Arrow: planning, optimisation, vectorised execution.
- [wasmtime](https://github.com/bytecodealliance/wasmtime): runtime and compiler pipeline; [Cranelift source](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift).
