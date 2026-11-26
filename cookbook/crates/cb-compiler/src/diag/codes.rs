//! The catalogue of diagnostic codes. `docs/ERRORS.md` is generated from this
//! table plus the golden UI tests (see `tests/ui.rs`), so each code has an example.
//!
//! Numbering: `E00xx` lexer, `E01xx` parser, `E02xx` names and declarations,
//! `E03xx` types, `E04xx` patterns, `W05xx` warnings.

use super::Severity;

pub struct CodeInfo {
    pub code: &'static str,
    pub severity: Severity,
    pub title: &'static str,
    pub explanation: &'static str,
}

const fn e(code: &'static str, title: &'static str, explanation: &'static str) -> CodeInfo {
    CodeInfo {
        code,
        severity: Severity::Error,
        title,
        explanation,
    }
}
const fn w(code: &'static str, title: &'static str, explanation: &'static str) -> CodeInfo {
    CodeInfo {
        code,
        severity: Severity::Warning,
        title,
        explanation,
    }
}

pub const CODES: &[CodeInfo] = &[
    e(
        "E0001",
        "invalid character",
        "The lexer met a character that cannot start any token.",
    ),
    e(
        "E0002",
        "unterminated string literal",
        "A string literal reached the end of the line without a closing quote. Strings cannot span lines; use `\\n`.",
    ),
    e(
        "E0003",
        "unterminated block comment",
        "A `/*` comment was never closed. Block comments nest, so every `/*` needs its own `*/`.",
    ),
    e(
        "E0004",
        "invalid escape sequence",
        "Only `\\n \\t \\r \\0 \\\\ \\\" \\u{...}` are valid escapes in string literals.",
    ),
    e(
        "E0005",
        "integer literal too large",
        "Integer literals must fit in a signed 64-bit `int`.",
    ),
    e(
        "E0006",
        "malformed number literal",
        "A number was directly followed by letters, e.g. `12abc`.",
    ),
    e(
        "E0101",
        "unexpected token",
        "The parser expected a specific token (or one of several) and found something else.",
    ),
    e(
        "E0102",
        "expected expression",
        "An expression was required here.",
    ),
    e("E0103", "expected type", "A type was required here."),
    e("E0104", "expected pattern", "A pattern was required here."),
    e(
        "E0105",
        "expected item",
        "Only `fn`, `struct`, `enum` and `const` items may appear at the top level.",
    ),
    e(
        "E0106",
        "unclosed delimiter",
        "An opening `(`, `[` or `{` has no matching closing delimiter.",
    ),
    e(
        "E0107",
        "`let` without initialiser",
        "Every `let` binding must be initialised: `let x = value;`.",
    ),
    e(
        "E0109",
        "comparison operators cannot be chained",
        "`a < b < c` is ambiguous; parenthesise or use `&&`.",
    ),
    e(
        "E0112",
        "unexpected closing delimiter",
        "A `)`, `]` or `}` has no matching opening delimiter.",
    ),
    e(
        "E0113",
        "nesting too deep",
        "The parser refuses to nest more than 100 levels (or chain more than 300 operators), to keep later passes within stack limits.",
    ),
    e(
        "E0201",
        "unresolved name",
        "No variable, function or constant with this name is in scope.",
    ),
    e(
        "E0202",
        "unknown type",
        "No type with this name exists. Built-in types are `int`, `bool`, `str`.",
    ),
    e(
        "E0203",
        "unknown field",
        "The struct has no field with this name.",
    ),
    e(
        "E0204",
        "unknown variant",
        "The enum has no variant with this name.",
    ),
    e(
        "E0205",
        "duplicate definition",
        "Two items of the same kind (value or type) share a name.",
    ),
    e(
        "E0206",
        "duplicate field or variant",
        "A struct has two fields (or an enum two variants) with the same name.",
    ),
    e(
        "E0207",
        "duplicate parameter or generic",
        "Two parameters or two type parameters of one item share a name.",
    ),
    e(
        "E0208",
        "missing `main`",
        "A program must define `fn main()`.",
    ),
    e(
        "E0209",
        "invalid `main` signature",
        "`main` takes no parameters and returns `()`.",
    ),
    e(
        "E0210",
        "wrong number of type arguments",
        "A generic type was given too many or too few type arguments.",
    ),
    e(
        "E0212",
        "name bound twice in one pattern",
        "A pattern may bind each name only once.",
    ),
    e(
        "E0213",
        "or-pattern alternatives bind different names",
        "Every alternative of `A | B` must bind exactly the same set of names.",
    ),
    e(
        "E0214",
        "`break`/`continue` outside of a loop",
        "`break` and `continue` only make sense inside `while` or `loop`.",
    ),
    e(
        "E0215",
        "`return` outside of a function",
        "`return` cannot be used in a `const` initialiser.",
    ),
    e(
        "E0216",
        "cyclic constant",
        "Constants may not depend on themselves, directly or through other constants.",
    ),
    e(
        "E0217",
        "expected value, found type",
        "A struct or enum name was used where a value is needed.",
    ),
    e(
        "E0218",
        "built-in used as a value",
        "Built-in functions can only be called, not passed around.",
    ),
    e(
        "E0219",
        "`break` with a value in a `while` loop",
        "Only `loop` can produce a value with `break value`.",
    ),
    e(
        "E0301",
        "mismatched types",
        "The type of an expression does not match what the context requires.",
    ),
    e(
        "E0302",
        "wrong number of arguments",
        "A call supplied more or fewer arguments than the function declares.",
    ),
    e(
        "E0303",
        "expected a function",
        "Only function values can be called.",
    ),
    e(
        "E0304",
        "invalid operands for binary operator",
        "The operator is not defined for these operand types.",
    ),
    e(
        "E0305",
        "invalid operand for unary operator",
        "`-` needs an `int` and `!` needs a `bool`.",
    ),
    e(
        "E0306",
        "type has no fields",
        "Field access needs a struct (`.name`) or a tuple (`.0`).",
    ),
    e(
        "E0307",
        "cannot index this type",
        "Only arrays `[T]` can be indexed, with an `int`.",
    ),
    e(
        "E0308",
        "unknown method",
        "Method syntax is sugar for the built-ins `len`, `push`, `pop`, `to_str` and `chars`.",
    ),
    e(
        "E0309",
        "invalid cast",
        "`as` only converts `bool` to `int`.",
    ),
    e(
        "E0310",
        "type annotations needed",
        "The type of this expression could not be inferred.",
    ),
    e(
        "E0311",
        "infinite type",
        "Unification would create a type that contains itself.",
    ),
    e(
        "E0312",
        "missing fields in struct literal",
        "Every field of a struct must be given a value.",
    ),
    e(
        "E0313",
        "field initialised twice",
        "A struct literal names the same field more than once.",
    ),
    e(
        "E0314",
        "wrong kind of enum variant",
        "A variant was used as if it were a different kind (unit, tuple or struct).",
    ),
    e(
        "E0316",
        "assignment to immutable binding",
        "Only `let mut` bindings (and `mut` parameters) can be assigned to or mutated.",
    ),
    e(
        "E0317",
        "invalid assignment target",
        "The left side of `=` must be a variable, field, tuple field or index expression.",
    ),
    e(
        "E0318",
        "tuple index out of range",
        "The tuple has fewer elements than the index requires.",
    ),
    e(
        "E0322",
        "wrong number of sub-patterns",
        "A pattern lists a different number of elements than the tuple or variant has.",
    ),
    e(
        "E0324",
        "unsupported operand type for built-in",
        "The built-in cannot be used with a value of this type.",
    ),
    e(
        "E0401",
        "non-exhaustive patterns",
        "A `match` does not cover every possible value.",
    ),
    e(
        "E0402",
        "refutable pattern in `let`",
        "A `let` pattern must match every value of its type.",
    ),
    w(
        "W0501",
        "unused variable",
        "A variable or parameter is never read. Prefix the name with `_` to silence.",
    ),
    w(
        "W0502",
        "unused function",
        "A function is never reachable from `main`.",
    ),
    w(
        "W0503",
        "unreachable code",
        "Code follows an expression that never completes (`return`, `break`, `continue`, ...).",
    ),
    w(
        "W0504",
        "unreachable pattern",
        "An earlier arm already matches every value this arm could match.",
    ),
    w(
        "W0505",
        "shadowed variable",
        "A `let` hides an earlier variable of the same name. Off by default (`--warn-shadow`).",
    ),
    w(
        "W0506",
        "unused `mut`",
        "A variable is declared `mut` but never mutated.",
    ),
    w("W0507", "unused constant", "A constant is never used."),
    w(
        "W0508",
        "unused type",
        "A struct or enum is never mentioned.",
    ),
    w(
        "W0509",
        "unused value",
        "An expression statement computes a value and discards it.",
    ),
    w(
        "W0510",
        "`while true`",
        "`loop` expresses an infinite loop more directly.",
    ),
    w(
        "W0512",
        "redundant cast",
        "Casting an `int` to `int` does nothing.",
    ),
    w(
        "W0514",
        "binding named like an enum variant",
        "A bare capitalised name in a pattern is a new variable, not a variant. Variants are written `Enum::Variant`.",
    ),
    w(
        "W0516",
        "division by zero",
        "Dividing by the literal `0` always traps at run time.",
    ),
];

pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    CODES.iter().find(|c| c.code == code)
}
