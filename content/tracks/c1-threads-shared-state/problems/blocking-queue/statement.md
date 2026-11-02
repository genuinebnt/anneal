`BlockingQueue<T>` is the work queue between producer and consumer threads. It holds at most `cap` items.
`push` waits while the queue is full, `pop` waits while it is empty. `close()` ends the stream: pushes (also
ones already waiting) give their value back, pops drain what is queued and then return `None` instead of waiting
forever. `pop_timeout` waits at most the given time **in total**, whatever the wake-ups do.

Share it with `Arc` between any number of producers and consumers; every item pushed must be popped exactly once.
