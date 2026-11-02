Build the front end of a shell: `parse(line, env)` turns one command line into a pipeline of `Command`s
(`args`, an optional `stdin` file, an optional `stdout` redirect).

- Words are separated by spaces or tabs. `|` separates commands; `<`, `>`, `>>` redirect, and **need no surrounding spaces**
  (`a|b`, `cat<in>out`). A redirect's target is the next word. If a command has several redirects of the same kind, the last wins;
  they may appear anywhere in the command.
- `'...'` keeps every character literally. `"..."` keeps everything literally except `$` expansion and the escapes `\"`, `\\`
  and `\$` (any other backslash stays a backslash). Outside quotes a backslash makes the next character literal (`a\ b` is one
  word, `\|` is a pipe character). Adjacent pieces form one word: `a"b c"'d'` is `ab cd`. `""` is an empty word.
- `$NAME` (letters, digits, `_`, not starting with a digit) and `${NAME}` expand from `env`; an undefined name is the empty
  string; a `$` followed by anything else is a literal `$`. The result is **not** split into words, except that an unquoted
  expansion to the empty string adds no word at all (`echo $EMPTY x` has two arguments).
- `#` at the start of a word begins a comment that runs to the end of the line.
- An empty (or comment-only) line is `Ok(vec![])`. Errors: `UnterminatedQuote`; `EmptyCommand` for a stage without a command
  word (`| a`, `a |`, `a | | b`, `> f`); `MissingRedirectTarget` when `<`/`>`/`>>` has no word after it (including `a > | b` and `a > > f`);
  `BadVariable` for `${NAME` without `}`.
