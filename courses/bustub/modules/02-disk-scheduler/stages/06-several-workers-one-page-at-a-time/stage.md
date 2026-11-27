One worker serialises all the I/O. A real disk (an SSD especially) serves several requests at once, so one thread leaves most of its speed unused. More workers bring parallelism and one problem: requests for the **same page** must not overtake each other, or a read could miss the write before it. Your job is to get the parallelism without losing the order that matters.

> [!CHECK] You start four workers on one shared queue. A write of page 7 and then a read of page 7 are queued; two different workers take them at almost the same moment. What can go wrong? Describe a rule for which worker handles which request, so that the problem cannot happen however the threads are scheduled, and say what that rule costs you.
> ||The read can finish before the write, and see the old bytes: the order the *worker threads run* is not the order the requests were queued. The rule: send every request for a page to the same worker, chosen from the page id (a function of the page id alone), and let each worker serve its own queue in order. Requests for one page stay ordered; requests for different pages run in parallel. The cost is balance: if one hot page gets half the traffic, one worker is busy and the others idle.||
>
> - Which property must the routing function have for the guarantee to hold?
> - What is the relation between "per-page order" and "overall order"?
> - What happens to throughput if every request is for the same page?

## The task

`ShardedDiskScheduler::new(disk, workers)` starts `workers` worker threads, each with its own queue. `schedule(requests)` routes each request to one of the queues, based on its page.

- Requests for the **same page** are served in the order they were scheduled (by one caller), so a read after a write of the same page sees it, whatever the number of workers.
- Requests for **different pages** may be served in parallel and in any order.
- `workers == 0` is a bug in the caller: it **panics**.
- Dropping the scheduler finishes everything scheduled, stops every worker and joins them all, as for the single scheduler.

The property the tests check: **the answers do not depend on the number of workers.** A random sequence of reads and writes, run through 1, 2, 3 or 4 workers, gives the same answers as a plain model. A separate test measures that eight 100 ms writes to different pages on four workers take much less than 800 ms.

## Your freedom

How a page picks its worker, how queues and threads are stored, whether the new scheduler shares code with `DiskScheduler`. What you cannot change is the contract above.

## The Rust toolbox

**Routing with `rem_euclid`.** A page id is an `i32` and may be negative in a test. `page_id.0 % n` can be negative in Rust (and in C++) and would index out of bounds; `page_id.0.rem_euclid(n as i32) as usize` is always in `0..n`. A signed number to an index is the classic spot for an `as` bug; the *integers and casts* concept covers the rules.

**A `Vec` of queues and workers.** `Vec<Arc<Channel<Option<DiskRequest>>>>` holds one queue per worker; `Vec<JoinHandle<()>>` holds the threads. `(0..workers).map(|_| ...).collect::<Vec<_>>()` builds either.

**Reusing the worker body.** You already wrote what a worker does for a single queue; a function that takes `(queue, disk)` and loops can be started once per queue. Pass it clones of the `Arc`s.

**Asserting a precondition.** `assert!(workers > 0, "a sharded scheduler needs at least one worker")` panics with a message, which is what a caller error should do.

**Moving out of a `Vec` in `Drop`.** `for handle in self.handles.drain(..) { handle.join().unwrap(); }` empties the vector from `&mut self` and gives you owned handles; the same idea as `Option::take`, for a collection.

## If this is new

- [S3 Vec & slices](/t/s3-vec-slices): `Vec`, `iter().map().collect()`, `drain`.
- [L1 Ownership & moves](/t/l1-ownership-moves): moving each `Arc` clone into its worker's closure.
- [S1 Option & Result](/t/s1-option-result): the stop signal you used in 1b-04.
- The *integers and casts* concept (optional) for `as`, `rem_euclid` and `usize`.
- [C2 Message passing](/t/c2-message-passing): Use it; Understand it: a worker loop that stops when told, graceful shutdown with no lost jobs, a bounded channel from `Mutex` + `Condvar`.

## Tests

- Zero workers panics.
- Eight 100 ms writes to different pages on four workers finish in much less than 800 ms.
- Dropping finishes everything scheduled (idle too).
- For random sequences, any number of workers gives a model's answers.

## Hints

### The routing function

A function from page id to worker index that depends on nothing but the page id, so the same page always goes to the same worker. What is wrong with "pick the worker with the shortest queue"? What is wrong with "round-robin"?

### One queue per worker

If each worker has its own queue, then order within one queue is guaranteed by the queue. Which of your earlier pieces already gives you a queue and a worker loop, and can it be started more than once?

### Batches

`schedule(vec![w, r])` carries two requests, possibly for different pages and so different workers. Does the order *between* them matter? Does the order *within the same page* still hold?

## Performance

With `W` workers and independent pages the throughput scales close to `W` until the disk's own limit; with all traffic on one page it is the same as one worker. Real disks stop scaling at some queue depth. Contention on `schedule` is small: one lock per queue, not one for the whole scheduler.

**Measure it.** Over a disk that sleeps 100 microseconds per I/O, write 20 000 distinct pages with 1, 2, 4 and 8 workers, then 20 000 writes all to page 0. Plot or tabulate both lines and explain where they differ.

## Experiment

Optional. Predict first, then run.

1. **A bad router.** Route by a global counter (round-robin). Which test fails, how often, and what is the smallest sequence that fails (the proptest shrinks it for you)?
2. **Hot pages.** Send 90% of the writes to ten pages. How does the speed-up with 8 workers compare with the uniform case? What could a scheduler do about it without breaking per-page order?

## Other designs

- **A queue per worker, routed by page (ours).** Simple and deterministic.
- **A shared queue and a per-page lock.** Any worker takes any request but must hold that page's lock; the order within a page then depends on who locks first, which is not the order scheduled.
- **Work stealing.** An idle worker takes from a busy worker's queue; per-page order needs care.
- **`io_uring` with linked requests.** The kernel keeps the order of linked entries; very different code.

## In BusTub

BusTub's `DiskScheduler` has a single background thread, and the project does not ask for more. The sharded scheduler is this course's extension, built because several workers need a rule for keeping the order that one worker gets for free.

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `std::vector<std::thread>` | `Vec<JoinHandle<()>>` |
| `page_id % n` with `page_id` signed | `page_id.0.rem_euclid(n)`; `%` can be negative |
| a loop of `threads[i].join()` in the destructor | `for h in self.handles.drain(..) { h.join() }` in `Drop` |
| `assert(n > 0)` | `assert!(n > 0, "message")` |

**Port rule:** `%` on signed integers is a *remainder* in both languages, with the sign of the left operand; use `rem_euclid` when you need an index.

## Learn more

- [`i32::rem_euclid`](https://doc.rust-lang.org/std/primitive.i32.html#method.rem_euclid) · [`Vec::drain`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.drain)
- [`io_uring(7)`](https://man7.org/linux/man-pages/man7/io_uring.7.html), the Linux interface that moves the queueing into the kernel
