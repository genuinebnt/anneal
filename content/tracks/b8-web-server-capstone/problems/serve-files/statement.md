> **Spoiler before you open the editor.** The starter of this step contains reference code for steps 1 to 7. It is a stand-in, because the app cannot yet carry your own code from one step to the next. It solves those steps and much of the preparation parts before them, so solve those first. You may replace it with your own code; keep the public names.

**Capstone, step 8 of 10.** Write `serve_static(root, raw, request)`, which answers a request for the file named by `raw`, a still percent-encoded path relative to `root`. `http_date` is given. A route like `GET /static/*path` calls it.

**The path.** Percent-decode `raw` once. A `%` not followed by two hex digits, or a result that is not UTF-8, is 400. A decoded NUL or backslash is 403. Split at `/`; empty and `.` segments are ignored; any `..` segment is 403. A path that, after links are followed, lies outside the folder `root` resolves to is 403. A link that stays inside works. A missing path, or one that is not a file, is 404. Nothing outside the folder is ever sent.

**A found file** is `200` with `Content-Type` by extension (`txt`: `text/plain; charset=utf-8`, `html`: `text/html; charset=utf-8`, `json`: `application/json`, else `application/octet-stream`), `Accept-Ranges: bytes`, `Last-Modified: http_date(modification time)` and a quoted `ETag` that changes when the file's length or modification time does.

**Unchanged.** If the request has `If-None-Match` (a comma-separated list; a `W/` prefix is ignored; `*` matches) and it matches, answer 304 with the validator headers and no body. Only without `If-None-Match`, an `If-Modified-Since` equal to the current `Last-Modified` also gives 304. This comes before ranges.

**Ranges**, for GET only. `Range: bytes=a-b` (inclusive), `bytes=a-` or `bytes=-n` (the last n bytes). An end past the file is cut to it. A satisfiable range gives 206 with `Content-Range: bytes a-b/len`. A start at or past the end, `-0`, or any suffix of an empty file gives 416 with `Content-Range: bytes */len`. Anything else (several ranges, another unit, `b < a`, not digits) is ignored: 200. Numbers may be too large to fit; treat them as very large.
