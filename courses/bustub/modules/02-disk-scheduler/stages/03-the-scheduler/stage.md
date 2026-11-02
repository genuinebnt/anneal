You have a queue that waits and a promise that answers. Put them together: a `DiskScheduler` owns a thread that takes disk requests from the queue, performs them one at a time on a disk, and answers each caller through its promise. The caller's side stays short: schedule a request, do something else, wait on the future when the page is needed. The buffer pool you build next lives on exactly this.

> [!CHECK] BusTub's C++ request carries a raw `char *data` that the caller and the worker both point at, and the caller promises not to touch it until the future says it is done. Rust will not accept that arrangement. Describe what you would put in a request instead, so that nobody can touch the buffer while the worker is using it, and say how the buffer gets back to the caller.
> ||Give the request **ownership** of the buffer: a `Box<PageData>` moved into the request and then into the worker thread. While the worker has it nobody else can name it, so there is nothing to forget. When the I/O is finished the worker moves the buffer into the promise's value, and the caller gets it back from `get`. "Do not touch until done" becomes a rule the compiler checks.||
>
> - Who owns the buffer at each moment: the caller, the queue, the worker, the future?
> - What would the caller do with a read's result if the request kept the buffer?
> - What has to be `Send` for the request to cross into the worker thread?

## The task

`DiskRequest::read(page_id)` and `DiskRequest::write(page_id, data)` each return a pair: the request, and the future of its result. `DiskResult` is `io::Result<Box<PageData>>`: the page that was read, or the page that was written handed back, or the disk's error.

`DiskScheduler::new(disk)` takes the disk as `Arc<dyn DiskIo>` and starts a background thread. Then:

- `schedule(requests)` hands the requests over **in order** and returns at once; it never does the I/O on the caller's thread.
- The worker runs the requests **one at a time, in the order they were scheduled**. So a read scheduled after a write of the same page sees that write, whichever thread scheduled them.
- A successful request completes its future with `Ok(buffer)`; a disk error completes it with `Err(error)`. Every request gets exactly one answer.
- `create_promise()` returns a fresh promise/future pair (BusTub has this for building requests by hand).

You do not need a clean shutdown yet; that is the next stage. Until then, wait on your futures and let the program end.

The property the tests check on random sequences: **scheduling is the same as running the requests one after another on a plain map of pages**, however the requests are split into batches.

## Your freedom

The fields of `DiskRequest` and `DiskScheduler`, how the worker is started and how it loops, and how results travel. You can keep the `Channel<Option<DiskRequest>>` you built, or use another queue. Add private helpers. Constructors and the five public methods are the only things the tests call.

## The Rust toolbox

**Ownership as the protocol.** A struct that holds `Box<PageData>` and a `Promise<DiskResult>` can be moved to another thread as a whole, and nobody else keeps a reference to its parts. This is the Rust replacement for "caller promises not to touch the pointer".

**`thread::spawn(move || { ... })`.** The closure runs on a new thread. `move` transfers every variable the closure uses into it. Anything it uses must be `Send + 'static`, which is why you clone an `Arc` first (`let queue = Arc::clone(&self.queue);`) and move the clone in, rather than borrowing `self`. The message "closure may outlive the current function, but it borrows `self`" means you forgot the `move` or tried to borrow.

**`Arc<dyn DiskIo>`.** `dyn DiskIo` is "some disk, decided at run time"; `Arc<..>` lets the scheduler and its thread share it. To call through it from the worker: `disk.read_page(page_id, &mut data)?`, where `&mut data` turns a `Box<[u8; N]>` into `&mut [u8; N]` by deref.

**Destructuring to move fields out.** `let DiskRequest { page_id, mut data, callback, .. } = request;` takes the request apart so each field can be moved or mutated separately. The compiler allows this because `DiskRequest` has no `Drop` of its own.

**Map a result without matching.** `result.map(|()| data)` turns `io::Result<()>` into `io::Result<Box<PageData>>` by attaching the buffer on success, which is the shape the future wants. `Result::map` and `map_err` save a lot of `match` boilerplate (see *errors as values* if they are new).

## If this is new

- [L1 Ownership & moves](/t/l1-ownership-moves): moving a value into a struct and into a closure.
- [L2 Borrowing](/t/l2-borrowing): `&mut` through a `Box`, and why two threads cannot both hold one.
- [L4 Traits & dispatch](/t/l4-traits-dispatch): `dyn Trait` and why it sits behind a pointer.
- [S1 Option & Result](/t/s1-option-result): `map`, `?`, and `io::Result`.
- S7 Smart pointers & interior mutability: Use it; Understand it: `Arc`, why `Rc` cannot cross threads, shared state with two owners.
- L6 Closures & functional Rust: Closure basics; Fn / FnMut / FnOnce: `move` closures, a callback that mutates state.

## Tests

- A read scheduled after a write of the same page sees it; an unwritten page reads as zeros; a write hands its buffer back.
- `schedule` returns before a slow disk has finished.
- `create_promise` gives a connected pair.
- For random sequences of reads and writes, split into random batches, every read gives what a model of the pages says.

## Hints

### One thread, one queue, in order

If requests are taken from a queue by one worker, one at a time, the order is the queue's order. Which part of the design makes "a read after a write sees the write" true: the queue, the worker, or the disk? Write the answer down; the sharded scheduler (1b-06) will have to give up exactly this and win it back another way.

### What does the worker do with a request?

Say it in four steps: take the request out of the queue; do the I/O in the way the request asks; put the outcome in the promise; wait for the next request. Where does the buffer go in step three on success, and what goes there on failure?

### The compiler complains about the closure

"`dyn DiskIo` cannot be sent between threads safely" means the trait object needs `Send + Sync`, which the trait already requires; "the trait bound ... `Sized`" means you are holding a `dyn` value where a pointer is needed. Reach for `Arc<dyn DiskIo>` and clone the `Arc`.

## Performance

Scheduling a request costs a heap allocation (the promise's shared slot), a lock, a push and a wake: about a microsecond. Doing it from the caller's thread would cost the I/O itself, tens of microseconds to milliseconds. A batch of requests (`schedule(vec![..])`) pays for the wakeup once.

**Measure it.** Put a disk behind the scheduler that sleeps 100 microseconds per operation and write 1 000 pages: first waiting on each future before scheduling the next, then scheduling all and waiting at the end. The times differ by what? Predict before you run.

## Experiment

Optional. Predict first, then run.

1. **Two workers.** Start two worker threads over the same queue. Which of the tests fail, and which interleaving makes the first one fail? Keep the answer: it is why the sharded scheduler routes by page.
2. **A shared buffer.** Write a variant where the request carries an `Arc<Mutex<PageData>>` instead of owning a `Box`. What does the caller now have to be careful about, and what does the compiler no longer check for you?

## Other designs

- **One worker, one `Channel` (ours).** Simple, ordered, the I/O is serialised.
- **`std::sync::mpsc`.** Same shape; the receiver is single-consumer, which is exactly what one worker needs.
- **An `async` runtime with a blocking pool.** The scheduler becomes a function that returns a future; you would use `spawn_blocking`.
- **`io_uring`.** The kernel does the queueing and several I/Os are in flight at once; a big design change, and what some real engines do.

## In BusTub

```cpp
struct DiskRequest {
  bool is_write_;
  char *data_;                   // the caller's buffer, shared by convention
  page_id_t page_id_;
  std::promise<bool> callback_;
};
void DiskScheduler::Schedule(std::vector<DiskRequest> requests);
void DiskScheduler::StartWorkerThread();  // loop: get a request, run it, set callback_
```

## The C/C++ way

| C / C++ | Rust |
|---|---|
| `char *data_` shared with a promise "do not touch until done" | `Box<PageData>` owned by the request and handed back through the future |
| `std::promise<bool> callback_` | `Promise<DiskResult>` |
| `std::optional<DiskRequest>` as the queue's element | `Option<DiskRequest>` |
| `std::thread` member started in the constructor | `JoinHandle<()>` field set in `new` |
| `std::shared_ptr<DiskManager>` | `Arc<dyn DiskIo>` |

**Port rule:** a raw buffer pointer passed to another thread becomes a buffer the receiving thread owns, returned in the result.

## Learn more

- [`thread::spawn`](https://doc.rust-lang.org/std/thread/fn.spawn.html) · [`JoinHandle`](https://doc.rust-lang.org/std/thread/struct.JoinHandle.html) · [`Result::map`](https://doc.rust-lang.org/std/result/enum.Result.html#method.map)
- The Rust Book: [using threads](https://doc.rust-lang.org/book/ch16-01-threads.html)
