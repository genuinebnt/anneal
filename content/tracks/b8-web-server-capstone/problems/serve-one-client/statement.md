> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 4. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 5 of 10.** Put the server on the network. `Server::start(config, app)` listens on a free port of 127.0.0.1 and serves on background threads; `addr()` tells where. The tests do not stop it yet (that is step 10).

- Read until `parse_request` (with `config.limits`) has a whole request, however the bytes are split. Handle it with `app.handle`, send the response with `Connection: close`, and close.
- For a HEAD request send no body.
- An HTTP/1.0 client cannot read chunks: send it a streamed body with `Content-Length` instead.
- A bad request gets `error_response` and the connection closes.
- A client that goes away before its request is complete gets no reply.
- Every reply the server sends must reach a client that reads it, followed by a normal end of stream, **even if the client sent more bytes than the server read** (up to at least 60 KB).
- Nothing one client does, and no panicking handler, may stop the server from serving the next one.

One client at a time is enough in this step.
