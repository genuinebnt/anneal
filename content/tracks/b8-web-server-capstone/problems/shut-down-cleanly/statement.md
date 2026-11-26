> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 9. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 10 of 10: the whole core server.** Add `Server::shutdown(self)`; dropping a `Server` does the same. All tests of the earlier steps run here too.

When shutdown starts:
- new connections are refused soon, even while requests are still being handled;
- requests being handled are finished, and their responses say `Connection: close`;
- connections already accepted and waiting for a worker are served: a request that has arrived is answered, with `close`;
- idle connections are closed promptly, whatever `idle_timeout` is;
- a request that is only partly received may still complete, within `request_timeout`. A client that keeps dripping bytes cannot delay shutdown beyond that.

`shutdown` returns only when every thread the server started has ended; after it returns, the server holds nothing of the `App`.

**Optional afterwards** (not graded): a thread-per-connection mode, a load generator that reports throughput and latency percentiles, and an async server reusing your parser and router.
