A trading page shows prices as they change. Asking "anything new?" every second is wasteful and always late. The server should keep one response open and send each price as it happens.

Browsers have a built-in format for this: **Server-Sent Events** (`text/event-stream`). The body is a series of events separated by blank lines:

```
event: price
id: 7
data: {"symbol":"ABC","cents":1250}
```

The starter gives you `Prices`, a live feed built on `tokio::sync::broadcast`. Read it first. Each subscriber has a limited buffer, and one that falls behind loses the oldest quotes.

Write `router(prices)` with one route, **`GET /prices`**, optionally `GET /prices?symbol=ABC`. It answers 200 with an event stream.

- The stream carries every quote published **after the response has been returned**, even if the client has not started reading the body yet.
- Each quote is one event. `event` is `price`, `id` is the quote's `seq`, and `data` is JSON with exactly `symbol` and `cents`.
- With `?symbol=`, only quotes with exactly that symbol are sent. Symbols may contain any characters, quotes and line breaks included.
- Each event can be read **as soon as** its quote is published. The response does not wait for the feed to end.
- When this client has **fallen behind** and quotes were dropped, send one event with `event: missed` whose `data` is the number dropped. It has no `id`. Then carry on with the oldest quote still buffered. The number counts every dropped quote, whatever the symbol filter.
- When the feed is closed, send what is still buffered, then **end the body**. A request made after the close gets 200 with an empty body.
- While nothing happens, the connection must not look dead. Send an SSE **comment** (a line starting with `:`) at least every 15 seconds of quiet. Quiet means that nothing is being sent to **this** client, even while other symbols are busy.
- Several clients may listen at once, each at its own speed.
