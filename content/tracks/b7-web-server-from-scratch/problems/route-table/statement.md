A service maps each incoming method and path to the thing that should deal with it. Some paths contain values, such as a user's number, and some lead to a whole tree of files.

`Table<T>` stores a value of type `T` for each route.

**`add(method, pattern, value)`.** A pattern starts with `/` and is split at `/` into segments. A segment is

- literal text, which must be equal;
- `:name`, which matches one segment that is not empty and captures it under `name`;
- `*name`, only as the last segment, which matches the rest of the path (zero or more segments) and captures them joined with `/`.

**`find(method, path)`.** `path` may carry a query: everything from the first `?` is ignored. The path is split the same way, so `/a` and `/a/` are different. A pattern matches only the whole path.

- The first route added whose pattern **and** method both match wins: `Found::Route(&value, params)`.
- For method `HEAD`, if no `HEAD` route matches, the first matching `GET` route is used.
- If some patterns match but no method does, `Found::WrongMethod(allowed)`: the methods of the matching routes, each once, in the order the routes were added.
- Otherwise `Found::Nothing`.

`Params::get(name)` returns a captured value. `Params::len` is the number of captures. Methods are compared exactly.
