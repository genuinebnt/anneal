A server that cannot be stopped cannot be deployed, restarted or tested properly. Stopping it badly is worse: replies get cut off, threads stay behind, and the program holds on to things it should have released.

`Server::start(answer)` listens on a free port of 127.0.0.1 and serves **every client at the same time**, a thread for each. For each line a client sends (ending in `\n`, which is not part of it), the server sends `answer(line)` followed by `\n`. Lines are UTF-8. `addr()` says where it listens.

`stop(self)` shuts the server down and returns how many client connections it accepted. When it is called:

- no new connections are served, and soon connecting is refused;
- for lines already being answered, the replies are completed and sent;
- connections are then closed, including those of clients that are quiet, have sent half a line, or keep sending without ever finishing one; the clients see a normal end of stream;
- `stop` returns only when **every thread the server started has ended** and the server no longer holds `answer`.

Dropping a `Server` that was not stopped does the same. Stopping must work when no client ever connected. A client that disconnects, or an `answer` that panics, affects only that connection.

(You may assume clients read their replies, and that `answer` returns.)
