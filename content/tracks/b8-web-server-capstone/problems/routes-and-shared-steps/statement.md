> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 3. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 4 of 10.** Choose a handler for a request, and wrap all handlers in shared steps.

**Router.** `add(method, pattern, handler)`. A pattern is segments separated by `/`: literal text, `:name` (one non-empty segment) or, last only, `*name` (the rest of the path joined with `/`, possibly nothing). The whole path must match, and `/a` and `/a/` differ. `dispatch` uses the first route added whose pattern and method both match, handing the captured values to the handler as `Params`. For a HEAD request with no matching HEAD route, the first matching GET route is used. If only the pattern matches somewhere, answer 405 with an `Allow` header listing those routes' methods once each, in the order added, joined with `, `. Otherwise 404.

**App.** `App::new(router).wrap(a).wrap(b)`: `a` is outermost. A step gets the request and `next`; it may call `next` zero, one or several times, with this request or another, and may change the response. `handle` runs the chain. `App` is shared between threads.

- `catch_panics()`: a step that turns a panic in anything inside it into a 500 whose body does not repeat the panic message. Panics outside it are not its business.
- `request_ids()`: sets `X-Request-Id` on the response: the request's own value if it is 1 to 64 characters from `A-Z a-z 0-9 - _`, otherwise a new id starting `req-`, different each time.
