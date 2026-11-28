- `is_command`: does `input`, with surrounding whitespace ignored, name `command`, ignoring ASCII case?
- `count_word`: how many whitespace-separated words of `text` equal `word`, ignoring ASCII case?

Don't allocate: no `to_lowercase` or `to_uppercase`, and no `to_ascii_lowercase` either (it builds a new
`String` too). The tests count heap allocations.
