# The PEBBLE language

PEBBLE is a small, statically typed, expression-oriented language. A program is one file made of items and must define `fn main()`.

```pebble
enum Option<T> { None, Some(T) }

fn first<T>(xs: [T]) -> Option<T> {
    if len(xs) == 0 { Option::None } else { Option::Some(xs[0]) }
}

fn main() {
    match first([10, 20]) {
        Option::Some(v) => println("first = " + to_str(v)),
        Option::None => println("empty"),
    }
}
```

## Grammar (EBNF)

`{ x }` repetition, `[ x ]` optional, `|` alternative. Comments (`//`, nested `/* */`, doc `///`) and whitespace separate tokens.

```ebnf
module     = { item } ;
item       = fn_item | struct_item | enum_item | const_item ;
fn_item    = "fn" IDENT [ generics ] "(" [ param { "," param } [","] ] ")" [ "->" type ] block ;
param      = [ "mut" ] IDENT ":" type ;
generics   = "<" IDENT { "," IDENT } [","] ">" ;
struct_item= "struct" IDENT [ generics ] "{" [ field { "," field } [","] ] "}" ;
field      = IDENT ":" type ;
enum_item  = "enum" IDENT [ generics ] "{" [ variant { "," variant } [","] ] "}" ;
variant    = IDENT [ "(" type { "," type } ")" | "{" field { "," field } "}" ] ;
const_item = "const" IDENT ":" type "=" expr ";" ;

type       = IDENT [ "<" type { "," type } ">" ]      (* int bool str, struct/enum, type parameter *)
           | "(" [ type { "," type } ] ")"            (* () is the unit type; (T, U) a tuple *)
           | "[" type "]"                             (* growable array *)
           | "fn" "(" [ type { "," type } ] ")" [ "->" type ]
           | "!" ;

block      = "{" { stmt } [ expr ] "}" ;
stmt       = "let" pattern [ ":" type ] "=" expr ";"
           | expr ";"
           | block_like ;                              (* if / match / while / loop / block need no `;` *)

expr       = assign ;
assign     = or [ ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) assign ] ;
or         = and { "||" and } ;
and        = cmp { "&&" cmp } ;
cmp        = add [ ( "==" | "!=" | "<" | "<=" | ">" | ">=" ) add ] ;   (* non-associative *)
add        = mul { ( "+" | "-" ) mul } ;
mul        = cast { ( "*" | "/" | "%" ) cast } ;
cast       = unary { "as" type } ;
unary      = ( "-" | "!" ) unary | postfix ;
postfix    = primary { "(" args ")" | "[" expr "]" | "." IDENT [ "(" args ")" ] | "." INT } ;
primary    = INT | STRING | "true" | "false" | "()" | path
           | path "{" [ IDENT ":" expr { "," IDENT ":" expr } [","] ] "}"   (* struct literal *)
           | "(" expr ")" | "(" expr "," [ expr { "," expr } ] ")"           (* tuple *)
           | "[" [ expr { "," expr } [","] ] "]"
           | block | "if" expr block [ "else" ( block | if_expr ) ]
           | "match" expr "{" { pattern [ "if" expr ] "=>" expr [","] } "}"
           | "while" expr block | "loop" block
           | "break" [ expr ] | "continue" | "return" [ expr ] ;
path       = IDENT [ "::" IDENT ] ;                    (* Enum::Variant *)

pattern    = pat_atom { "|" pat_atom } ;
pat_atom   = "_" | [ "mut" ] IDENT | INT | "-" INT | STRING | "true" | "false" | "()"
           | "(" pattern { "," pattern } [","] ")"
           | path [ "(" pattern { "," pattern } ")" | "{" ( IDENT [ ":" pattern ] | ".." ) { "," ... } "}" ] ;
```

In the heads of `if`, `while` and `match`, `Name {` is never a struct literal (as in Rust); wrap it in parentheses if you need one there. A `match` arm whose body is a block needs no trailing comma.

Keywords: `fn struct enum const let mut if else match while loop break continue return as true false`.
Integer literals may contain `_` (`1_000`). String escapes: `\n \t \r \0 \\ \" \u{hex}`. Strings cannot span lines.

## Types and values

| Type | Notes |
|------|-------|
| `int` | 64-bit signed. Arithmetic **traps** on overflow, division/remainder by zero, `MIN / -1`. The minimum is written `-9223372036854775807 - 1` (the literal `9223372036854775808` is too large). |
| `bool`, `str` | `str` is immutable UTF-8; `+` concatenates, `==` and `< <= > >=` compare bytewise. `len(s)` is the byte length. |
| `()` | unit; the value is written `()`. |
| `[T]` | growable array with **reference semantics** (`let b = a;` aliases). Indexing is bounds-checked (trap). |
| `(T, U)` | tuples, projected with `.0`, `.1`. |
| `fn(T) -> U` | function values. There are no closures: any `fn` item can be used as a value. |
| structs, enums | nominal, may be generic. Structs, enums and tuples have *value semantics* (copy on write). |
| `!` | the never type (type of `return`, `break`, `continue`, `panic(..)`, `loop` without `break`). It is accepted anywhere a value is expected. |

### Generics

`fn map<T, U>(xs: [T], f: fn(T) -> U) -> [U]` and `enum Option<T> { None, Some(T) }`. Type arguments are never written at use sites: each use instantiates fresh inference variables which are solved by unification. Generic parameters are rigid inside their item, so `a + b` on `T` is an error (no bounds exist). The middle end will monomorphise.

### Enum variants

Variants are always written qualified: `Option::Some(1)`, `Shape::Rect { w: 1, h: 2 }`, `Color::Red`. In patterns a bare capitalised name is an ordinary binding (the checker warns, W0514).

## Expressions and statements

* `if`, `match`, blocks and `loop` are expressions. A block's value is its final expression; with a trailing `;` (or no tail) the block has type `()`. An `if` without `else` has type `()`.
* `loop { ... break value; }` yields `value`; `while` and `for`-less iteration use `while`. `break value` in a `while` is an error.
* `let` always initialises. Patterns in `let` must be irrefutable (`let (a, b) = pair;`). Shadowing is allowed.
* `let mut x` / `mut` parameters allow assignment `x = ..`, compound assignment, and mutation *through* the binding: `x.f = ..`, `x[i] = ..`, `push(x, ..)`, `pop(x)`.
* `&&`/`||` short-circuit.
* `as` converts only `bool` to `int` (`true as int == 1`). Use `n != 0` for the other direction. `int as int` is accepted with a warning.
* Method syntax is sugar for built-ins only: `xs.len()`, `xs.push(v)`, `xs.pop()`, `n.to_str()`, `s.chars()`.

### Built-ins

| Call | Type |
|------|------|
| `print(x)`, `println(x)`, `println()` | `x` is `int`, `bool`, `str` or `()`; returns `()` |
| `assert(cond)`, `assert(cond, msg)` | traps when false |
| `panic(msg: str)` | `!` |
| `to_str(x)` | `int`, `bool` or `str` to `str` |
| `len(x)` | `[T]` or `str` to `int` |
| `push(xs, v)`, `pop(xs)` | `pop` traps on an empty array |
| `chars(s)` | `str` to `[str]`, one element per Unicode scalar |

User items with the same name shadow built-ins.

### Patterns

Literals (`1`, `-1`, `"s"`, `true`), bindings (`x`, `mut x`), `_`, tuples, `Enum::Variant(p, ..)`, `Enum::Variant { f, g: p, .. }`, `Struct { f, .. }`, or-patterns `A | B` (every alternative must bind the same names), and guards `pat if cond =>`. Matches must be exhaustive; arms that can never run are reported (W0504). Guarded arms do not count towards exhaustiveness.

## Static semantics (typing rules, informally)

* **Inference** is Hindley-Milner style inside a function body: unification with an occurs check; no inference across function boundaries (signatures are explicit). Integer literals are always `int`, so inference variables only arise from generic instantiation and `[]`.
* **Application**: `f(e1..en)` requires `f : fn(T1..Tn) -> R` after instantiation and `ei : Ti` (up to `!`); the result is `R`.
* **Branches**: `if c {a} else {b}` needs `c : bool` and `a` and `b` to agree (a `!` branch adopts the other's type). `match` arms agree likewise.
* **Equality** `==`/`!=` works on any two values of one type that contains no function type; `<`... on two `int`s or two `str`s.
* **Assignment** targets are places rooted at a mutable local.
* **Definite assignment** is not needed: every `let` initialises.
* **Items**: duplicate names, duplicate fields/variants/parameters, arity of generic type applications, and `fn main()` (no parameters, returns `()`) are checked. Recursion and mutual recursion are fine (items are collected before bodies). Constants may not be cyclic.
* **Reachability warnings**: code after `return`/`break`/`continue`/diverging calls, unused variables, parameters, functions (not reachable from `main`), constants, types, unused `mut`, unused values, `while true`, redundant casts, shadowing (opt-in), division by literal zero.

## Runtime semantics

Evaluation is strict, left to right. Runtime traps (overflow, division by zero, bounds, empty `pop`, `assert`, `panic`) stop the program with a message and a stack trace. The reference interpreter limits recursion to 10 000 frames and execution to 200 million steps.
