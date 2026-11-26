> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 5. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 6 of 10.** `Config` gains `workers` (default 4) and `queue_cap` (default 16, at least 1).

- Up to `workers` connections are served at the same time. The number of threads must not grow with the number of clients.
- Up to `queue_cap` further connections wait. When all workers are busy and `queue_cap` connections are waiting, a new connection is answered `503 Service Unavailable` with `Retry-After: 1` and closed. Accepted connections are all served.
- As in step 5, the 503 must reach the client followed by a normal end of stream, whether its request was already on the way, is sent as the refusal arrives, or never comes. (A server cannot wait for ever for bytes sent much later.)
- A refused client must not keep the server from accepting and refusing others.
- A panic while serving a connection costs no worker.
