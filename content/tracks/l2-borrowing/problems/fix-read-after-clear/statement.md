`key_runs` reads lines into one reused `String`, the usual way to avoid an allocation per line. It
doesn't compile: the previous line's key is a slice of the buffer that `clear` and `read_line` are about
to overwrite. Fix it. Keep the single `read_line` buffer, and allocate once per run, not once per line:
a test counts allocations.
