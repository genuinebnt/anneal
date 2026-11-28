from author import T, write_track

P = []


def fix(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, rules=None, related=("L5",), source=None, wrong=None):
    return dict(slug=slug, title=title, mode="fix", level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, rules=rules, related=list(related), source=source, wrong=wrong)


def write(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, related=("L5",), source=None, examples=(), wrong=None):
    return dict(slug=slug, title=title, level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, related=list(related), source=source, examples=list(examples), wrong=wrong)



# ---------------------------------------------------------------- generic code (easy)
# The easy band is recall under pressure: each problem makes you write several generic constructs from memory.

STACK_CORE = """
use std::fmt;

/// A LIFO stack. Iterating yields items top first, the order `pop` would give them.
#[derive(Debug{derive_default})]
pub struct Stack<T> {{
    items: Vec<T>,
}}

impl<T> Stack<T> {{
    pub fn new() -> Self {{
        Stack {{ items: Vec::new() }}
    }}

    pub fn push(&mut self, item: T) {{
        self.items.push(item);
    }}

    pub fn pop(&mut self) -> Option<T> {{
        self.items.pop()
    }}

    pub fn peek(&self) -> Option<&T> {{
        self.items.last()
    }}

    pub fn len(&self) -> usize {{
        self.items.len()
    }}

    pub fn is_empty(&self) -> bool {{
        self.items.is_empty()
    }}
}}
"""


def stack_solution(own_iter="self.items.into_iter().rev()", own_ty="std::iter::Rev<std::vec::IntoIter<T>>",
                   ref_iter="self.items.iter().rev()", ref_ty="std::iter::Rev<std::slice::Iter<'a, T>>",
                   display_bound="fmt::Display", item_fmt='write!(f, "{x}")?;', display_iter="self"):
    return STACK_CORE.format(derive_default="") + f"""
// derive(Default) would add `T: Default`; an empty stack needs nothing from T.
impl<T> Default for Stack<T> {{
    fn default() -> Self {{
        Stack::new()
    }}
}}

/// Pushes in iteration order, so the last item ends up on top.
impl<T> FromIterator<T> for Stack<T> {{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {{
        Stack {{ items: iter.into_iter().collect() }}
    }}
}}

impl<T> Extend<T> for Stack<T> {{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {{
        self.items.extend(iter);
    }}
}}

impl<T> IntoIterator for Stack<T> {{
    type Item = T;
    type IntoIter = {own_ty};

    fn into_iter(self) -> Self::IntoIter {{
        {own_iter}
    }}
}}

impl<'a, T> IntoIterator for &'a Stack<T> {{
    type Item = &'a T;
    type IntoIter = {ref_ty};

    fn into_iter(self) -> Self::IntoIter {{
        {ref_iter}
    }}
}}

/// `[top, ..., bottom]`
impl<T: {display_bound}> fmt::Display for Stack<T> {{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{
        write!(f, "[")?;
        for (i, x) in {display_iter}.into_iter().enumerate() {{
            if i > 0 {{
                write!(f, ", ")?;
            }}
            {item_fmt}
        }}
        write!(f, "]")
    }}
}}
"""


STACK_TOKEN = """
/// No derives at all: not Default, Clone, Debug or Display.
struct Token(u32);
"""

P.append(fix(
    "stack-plugs-into-std", "Stack<T> that plugs into std", "easy", "generic-code", ["generic impls", "IntoIterator", "FromIterator", "derive bounds"],
    """
        `Stack<T>` works, but it doesn't fit in with std, so the tests don't compile. Make these work for **any** `T`
        (the tests use a `Token` type with no derives):

        - `Stack::default()`;
        - `collect()` into a `Stack` and `extend` one (items are pushed in order, so the last one ends up on top);
        - `for x in stack` and `for x in &stack`, both **top first**;
        - `{}` printing as `[top, ..., bottom]` whenever `T: Display`.
    """,
    STACK_CORE.format(derive_default=", Default") + """
// TODO: Default, FromIterator, Extend, IntoIterator (by value and by reference), Display.
""",
    stack_solution(),
    [STACK_TOKEN,
     T("collect_then_pop", "(1..=3).collect(), then pop", "(s.pop(), s.len())", "(Some(3), 2)", setup="let mut s: Stack<i32> = (1..=3).collect();"),
     T("into_iter_top_first", "collect [1, 2, 3], then into_iter", "s.into_iter().collect::<Vec<_>>()", "vec![3, 2, 1]", setup="let s: Stack<i32> = vec![1, 2, 3].into_iter().collect();"),
     T("borrowing_loop", "for x in &s over [1, 2, 3], then len", "(seen, s.len())", "(vec![3, 2, 1], 3)",
       setup="let s: Stack<i32> = vec![1, 2, 3].into_iter().collect();\nlet mut seen = Vec::new();\nfor x in &s {\n    seen.push(*x);\n}"),
     T("display", "[1, 2, 3] and an empty stack", "(s.to_string(), Stack::<i32>::new().to_string())", '("[3, 2, 1]".to_string(), "[]".to_string())',
       setup="let s: Stack<i32> = vec![1, 2, 3].into_iter().collect();"),
     T("default_for_any_t", "Stack::<Token>::default(), push Token(7)", "(s.len(), s.pop().map(|t| t.0))", "(1, Some(7))",
       setup="let mut s = Stack::<Token>::default();\ns.push(Token(7));")],
    [STACK_TOKEN,
     T("extend_pushes_in_order", "push 1, extend [2, 3], pop", "(s.pop(), s.len())", "(Some(3), 2)",
       setup="let mut s = Stack::new();\ns.push(1);\ns.extend(vec![2, 3]);"),
     T("display_strings", 'collect ["a", "b"] as Strings', "s.to_string()", '"[b, a]"',
       setup='let s: Stack<String> = vec!["a".to_string(), "b".to_string()].into_iter().collect();'),
     T("display_nested", "a Stack<Stack<i32>>: push [1, 2], then an empty stack", "s.to_string()", '"[[], [2, 1]]"',
       setup="let mut s: Stack<Stack<i32>> = Stack::new();\ns.push(vec![1, 2].into_iter().collect());\ns.push(Stack::new());"),
     T("empty_iterations", "empty Stack<i32>: by-ref and by-value iteration", "((&s).into_iter().count(), s.into_iter().count())", "(0, 0)",
       setup="let s: Stack<i32> = Stack::default();"),
     T("collect_empty", "collect an empty iterator", "(s.is_empty(), s.peek().is_none())", "(true, true)",
       setup="let s: Stack<u8> = std::iter::empty().collect();"),
     T("no_clone_needed", "Tokens: collect, iterate by ref, then by value", "(by_ref, by_val)", "(vec![3, 2, 1], vec![3, 2, 1])",
       setup="let s: Stack<Token> = (1..=3).map(Token).collect();\nlet by_ref: Vec<u32> = (&s).into_iter().map(|t| t.0).collect();\nlet by_val: Vec<u32> = s.into_iter().map(|t| t.0).collect();"),
     T("first_of_into_iter_is_peek", "collect \"xyz\" chars", "(s.peek().copied(), s.into_iter().next())", "(Some('z'), Some('z'))",
       setup="let s: Stack<char> = \"xyz\".chars().collect();"),
     T("extend_with_strings_by_ref_loop", "extend with words, then sum lengths by ref", "(total, s.len())", "(10, 3)",
       setup='let mut s: Stack<String> = Stack::new();\ns.extend("one three ab".split(\' \').map(String::from));\nlet mut total = 0;\nfor w in &s {\n    total += w.len();\n}'),
     T("drops_unconsumed_items", "Rc clones: take one from into_iter, drop the rest", "std::rc::Rc::strong_count(&rc)", "1",
       setup="let rc = std::rc::Rc::new(0);\nlet s: Stack<std::rc::Rc<i32>> = (0..3).map(|_| rc.clone()).collect();\nlet mut it = s.into_iter();\nlet first = it.next();\ndrop(it);\ndrop(first);"),
     """
     #[test]
     fn random_vs_vec_model() {
         let mut rng = anneal_prelude::Rng::new(4501);
         for _ in 0..300 {
             let n = rng.below(6);
             let start: Vec<i32> = rng.vec(n, -9, 9);
             let m = rng.below(4);
             let more: Vec<i32> = rng.vec(m, -9, 9);
             let mut s: Stack<i32> = start.clone().into_iter().collect();
             s.extend(more.clone());
             let mut model = start.clone();
             model.extend(more.clone());
             let top_first: Vec<i32> = model.iter().rev().copied().collect();
             let shown = format!("[{}]", top_first.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "));
             let by_ref: Vec<i32> = (&s).into_iter().copied().collect();
             let text = s.to_string();
             let by_val: Vec<i32> = s.into_iter().collect();
             check!(format!("collect {start:?}, extend {more:?}"), (by_ref, text, by_val), (top_first.clone(), shown, top_first));
         }
     }
     """],
    [("rust", "`#[derive(Default)]` expands to `impl<T: Default> Default for Stack<T>`. Write the impl by hand with no bound."),
     ("rust", "`IntoIterator` has two associated types. By value: `type IntoIter = Rev<vec::IntoIter<T>>`. By reference: `impl<'a, T> IntoIterator for &'a Stack<T>` with `type Item = &'a T`."),
     ("rust", "`FromIterator` and `Extend` each have one generic method: `fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self`.")],
    ("""Each trait is a generic impl on `Stack<T>`; only `Display` needs a bound, so only that impl has one. Syntax to remember:
`impl<T> FromIterator<T> for Stack<T> { fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self }`,
`impl<'a, T> IntoIterator for &'a Stack<T> { type Item = &'a T; type IntoIter = Rev<slice::Iter<'a, T>>; fn into_iter(self) -> Self::IntoIter }`,
`impl<T: fmt::Display> fmt::Display for Stack<T> { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result }`.""", "O(1) amortised per push; O(n) to iterate", "O(n)"),
    "Why does `derive(Clone)` on `struct Wrapper<T>(Rc<T>)` require `T: Clone`, and what would you write instead?",
    ["Derives add a bound on every type parameter; hand-written impls don't have to.", "`IntoIterator` for `Stack<T>` and for `&'a Stack<T>`, with their associated types.", "`FromIterator` / `Extend` make `collect()` and `extend()` work."],
    source="W1", related=("L5", "S6", "L4"),
    wrong=dict(
        bottom_first=stack_solution(own_iter="self.items.into_iter()", own_ty="std::vec::IntoIter<T>", ref_iter="self.items.iter()", ref_ty="std::slice::Iter<'a, T>"),
        display_bottom_first=stack_solution(display_iter="self.items.iter()"),
        debug_display=stack_solution(display_bound="fmt::Debug", item_fmt='write!(f, "{x:?}")?;'),
    ),
))

VEC2_HEAD = """
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2<T> {
    pub x: T,
    pub y: T,
}

impl<T> Vec2<T> {
    pub fn new(x: T, y: T) -> Self {
        Vec2 { x, y }
    }
}
"""


def vec2(trait_decl="pub trait Dot<Rhs> {", add="impl<T> Add for Vec2<T> {", sub="impl<T> Sub for Vec2<T> {", neg="impl<T> Neg for Vec2<T> {",
         mul="impl<T> Mul<T> for Vec2<T> {", mul_body="Vec2::new(self.x * k, self.y * k)", add_assign="impl<T> AddAssign for Vec2<T> {",
         sum="impl<T> Sum for Vec2<T> {", sum_body="iter.fold(Vec2::new(T::default(), T::default()), |acc, v| acc + v)",
         dot="impl<T> Dot for Vec2<T> {", dot_body="self.x * rhs.x + self.y * rhs.y"):
    return VEC2_HEAD + f"""
/// `a.dot(b)`: the dot product.
{trait_decl}
    type Output;
    fn dot(self, rhs: Rhs) -> Self::Output;
}}

{add}
    type Output = Vec2<T>;
    fn add(self, rhs: Self) -> Self::Output {{
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }}
}}

{sub}
    type Output = Vec2<T>;
    fn sub(self, rhs: Self) -> Self::Output {{
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }}
}}

{neg}
    type Output = Vec2<T>;
    fn neg(self) -> Self::Output {{
        Vec2::new(-self.x, -self.y)
    }}
}}

/// Scales both parts: `v * k`.
{mul}
    type Output = Vec2<T>;
    fn mul(self, k: T) -> Self::Output {{
        {mul_body}
    }}
}}

{add_assign}
    fn add_assign(&mut self, rhs: Self) {{
        self.x += rhs.x;
        self.y += rhs.y;
    }}
}}

/// The sum of no vectors is (0, 0).
{sum}
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {{
        {sum_body}
    }}
}}

{dot}
    type Output = T;
    fn dot(self, rhs: Self) -> T {{
        {dot_body}
    }}
}}
"""


VEC2_OK = dict(trait_decl="pub trait Dot<Rhs = Self> {", add="impl<T: Add<Output = T>> Add for Vec2<T> {", sub="impl<T: Sub<Output = T>> Sub for Vec2<T> {",
               neg="impl<T: Neg<Output = T>> Neg for Vec2<T> {", mul="impl<T: Mul<Output = T> + Copy> Mul<T> for Vec2<T> {",
               add_assign="impl<T: AddAssign> AddAssign for Vec2<T> {", sum="impl<T: Add<Output = T> + Default> Sum for Vec2<T> {",
               dot="impl<T: Mul<Output = T> + Add<Output = T>> Dot for Vec2<T> {")

BIG = """
/// A number that is not Copy or Clone.
#[derive(Debug, PartialEq, Default)]
struct Big(i128);

impl std::ops::Add for Big {
    type Output = Big;
    fn add(self, o: Big) -> Big {
        Big(self.0 + o.0)
    }
}

impl std::ops::Sub for Big {
    type Output = Big;
    fn sub(self, o: Big) -> Big {
        Big(self.0 - o.0)
    }
}

impl std::ops::Neg for Big {
    type Output = Big;
    fn neg(self) -> Big {
        Big(-self.0)
    }
}

impl std::ops::AddAssign for Big {
    fn add_assign(&mut self, o: Big) {
        self.0 += o.0;
    }
}

impl std::ops::Mul for Big {
    type Output = Big;
    fn mul(self, o: Big) -> Big {
        Big(self.0 * o.0)
    }
}

/// Works for any type whose dot product is with itself: needs `Dot`'s default type parameter.
fn len_sq<V: Dot + Copy>(v: V) -> V::Output {
    v.dot(v)
}
"""

P.append(fix(
    "fix-vec2-operators", "Fix: operators on Vec2<T> (E0369)", "easy", "generic-code", ["E0369", "operator traits", "Output = T", "default type parameters"],
    """
        `Vec2<T>` implements `+`, `-`, unary `-`, `v * k`, `+=`, `.sum()` and `a.dot(b)`, but none of it compiles. Add the
        bounds each impl needs, and no more: the tests use a number type that is neither `Copy` nor `Clone`. The tests
        also write the bound `V: Dot`, with no type argument.
    """,
    vec2(),
    vec2(**VEC2_OK),
    [BIG,
     T("add", "(1, 2) + (3, 4)", "Vec2::new(1, 2) + Vec2::new(3, 4)", "Vec2::new(4, 6)"),
     T("scale", "(1.5, -2.0) * 2.0", "Vec2::new(1.5, -2.0) * 2.0", "Vec2::new(3.0, -4.0)"),
     T("neg_and_sub", "-(1, 2) - (3, 4)", "-Vec2::new(1, 2) - Vec2::new(3, 4)", "Vec2::new(-4, -6)"),
     T("sum", "[(1, 1), (2, 3), (-1, 0)].sum()", "vec![Vec2::new(1, 1), Vec2::new(2, 3), Vec2::new(-1, 0)].into_iter().sum::<Vec2<i32>>()", "Vec2::new(2, 4)"),
     T("dot_with_default_rhs", "len_sq((3, 4)), where len_sq takes V: Dot", "len_sq(Vec2::new(3, 4))", "25"),
     T("no_copy_needed", "Big values: (a + b - c), then += and neg", "-v",
       "Vec2::new(Big(-6), Big(-9))", setup="let mut v = Vec2::new(Big(1), Big(2)) + Vec2::new(Big(3), Big(4)) - Vec2::new(Big(0), Big(1));\nv += Vec2::new(Big(2), Big(4));")],
    [BIG,
     T("add_assign", "(1, 2) += (10, 20)", "v", "Vec2::new(11, 22)", setup="let mut v = Vec2::new(1, 2);\nv += Vec2::new(10, 20);"),
     T("empty_sum_is_zero", "an empty iterator of Vec2<f64>", "std::iter::empty::<Vec2<f64>>().sum::<Vec2<f64>>()", "Vec2::new(0.0, 0.0)"),
     T("dot_floats", "(0.5, 2.0) . (4.0, -1.0)", "Vec2::new(0.5, 2.0).dot(Vec2::new(4.0, -1.0))", "0.0"),
     T("wrapping", "Wrapping(250u8) + Wrapping(10u8) per part", "Vec2::new(Wrapping(250u8), Wrapping(1u8)) + Vec2::new(Wrapping(10u8), Wrapping(1u8))",
       "Vec2::new(Wrapping(4u8), Wrapping(2u8))", setup="use std::num::Wrapping;"),
     T("big_sum", "Big values summed", "vec![Vec2::new(Big(1), Big(2)), Vec2::new(Big(3), Big(4))].into_iter().sum::<Vec2<Big>>()", "Vec2::new(Big(4), Big(6))"),
     T("big_dot", "Big (2, 3) . (4, 5)", "Vec2::new(Big(2), Big(3)).dot(Vec2::new(Big(4), Big(5)))", "Big(23)"),
     T("scale_by_zero", "(7, -7) * 0", "Vec2::new(7, -7) * std::hint::black_box(0)", "Vec2::new(0, 0)"),
     T("nested_vectors", "Vec2<Vec2<i32>> addition", "Vec2::new(Vec2::new(1, 2), Vec2::new(3, 4)) + Vec2::new(Vec2::new(10, 20), Vec2::new(30, 40))",
       "Vec2::new(Vec2::new(11, 22), Vec2::new(33, 44))"),
     T("dot_negative", "(-2, 3) . (4, 5)", "Vec2::new(-2, 3).dot(Vec2::new(4, 5))", "7"),
     T("i64_extremes", "(i64::MAX - 1, 0) + (1, i64::MIN)", "Vec2::new(i64::MAX - 1, 0) + Vec2::new(1, i64::MIN)", "Vec2::new(i64::MAX, i64::MIN)"),
     """
     #[test]
     fn random_vs_tuples() {
         let mut rng = anneal_prelude::Rng::new(4502);
         for _ in 0..300 {
             let (ax, ay, bx, by, k) = (rng.int(-99, 99), rng.int(-99, 99), rng.int(-99, 99), rng.int(-99, 99), rng.int(-9, 9));
             let (a, b) = (Vec2::new(ax, ay), Vec2::new(bx, by));
             let mut c = a;
             c += b;
             check!(format!("a = ({ax}, {ay}), b = ({bx}, {by}), k = {k}"),
                    (a + b, a - b, -a, a * k, c, vec![a, b, a].into_iter().sum::<Vec2<i64>>(), a.dot(b), len_sq(b)),
                    (Vec2::new(ax + bx, ay + by), Vec2::new(ax - bx, ay - by), Vec2::new(-ax, -ay), Vec2::new(ax * k, ay * k),
                     Vec2::new(ax + bx, ay + by), Vec2::new(2 * ax + bx, 2 * ay + by), ax * bx + ay * by, bx * bx + by * by));
         }
     }
     """],
    [("rust", "`T: Add` isn't enough: `x + y` then has type `<T as Add>::Output`, not `T`. Pin it with `T: Add<Output = T>`."),
     ("rust", "Only `v * k` uses a value twice (`k`), so only that impl needs `Copy`. `+=` needs `AddAssign`, `.sum()` needs a zero: `Default`."),
     ("rust", "`V: Dot` with no argument only works if the trait declares a default: `trait Dot<Rhs = Self>`.")],
    ("""Each impl asks for exactly what its body does. Syntax to remember: `impl<T: Add<Output = T>> Add for Vec2<T>` (associated-type binding in a bound); `trait Dot<Rhs = Self>` (default type parameter, as std's `Add<Rhs = Self>`); `impl<T: Mul<Output = T> + Copy> Mul<T> for Vec2<T>` (a non-default `Rhs`). Adding `Copy` everywhere would compile, but it would leak into every caller and shut out `Big`, `BigInt` and friends.""", "O(1) per operation", "O(1)"),
    "Why is `Add` declared as `trait Add<Rhs = Self>` with an associated `Output`, instead of `trait Add<Rhs, Output>`?",
    ["Operator traits: `Add<Output = T>` bindings.", "Default type parameters (`Rhs = Self`).", "The weakest bound per impl; don't make every caller pay for `Copy`."],
    related=("L5", "L4"),
    wrong=dict(
        cross_not_dot=vec2(**{**VEC2_OK, "dot_body": "self.x * rhs.y + self.y * rhs.x"}),
        scales_only_x=vec2(**{**VEC2_OK, "mul_body": "Vec2::new(self.x * k, self.y)"}),
        sum_without_zero=vec2(**{**VEC2_OK, "sum": "impl<T: Add<Output = T>> Sum for Vec2<T> {", "sum_body": "iter.reduce(|acc, v| acc + v).expect(\"empty sum\")"}),
    ),
))


def parse_list(generic=True, index="index", source_body="Some(source)", display_item='write!(f, "item {index}: {source}")'):
    if not generic:
        return """
use std::error::Error;
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub enum ParseListError {
    /// Nothing but whitespace.
    Empty,
    /// The item at `index` (0-based) didn't parse.
    Bad { index: usize, source: ParseIntError },
}

impl fmt::Display for ParseListError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseListError::Empty => write!(f, "empty list"),
            ParseListError::Bad { index, source } => write!(f, "item {index}: {source}"),
        }
    }
}

impl Error for ParseListError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ParseListError::Empty => None,
            ParseListError::Bad { source, .. } => Some(source),
        }
    }
}

/// Parses comma-separated items, trimming each one.
pub fn parse_list(s: &str) -> Result<Vec<i32>, ParseListError> {
    if s.trim().is_empty() {
        return Err(ParseListError::Empty);
    }
    s.split(',')
        .enumerate()
        .map(|(index, item)| item.trim().parse().map_err(|source| ParseListError::Bad { index, source }))
        .collect()
}
"""
    return f"""
use std::error::Error;
use std::fmt;
use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub enum ParseListError<E> {{
    /// Nothing but whitespace.
    Empty,
    /// The item at `index` (0-based) didn't parse.
    Bad {{ index: usize, source: E }},
}}

impl<E: fmt::Display> fmt::Display for ParseListError<E> {{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{
        match self {{
            ParseListError::Empty => write!(f, "empty list"),
            ParseListError::Bad {{ index, source }} => {display_item},
        }}
    }}
}}

// `source` hands out `&(dyn Error + 'static)`, so E must be 'static too.
impl<E: Error + 'static> Error for ParseListError<E> {{
    fn source(&self) -> Option<&(dyn Error + 'static)> {{
        match self {{
            ParseListError::Empty => None,
            ParseListError::Bad {{ source, .. }} => {source_body},
        }}
    }}
}}

/// Parses comma-separated items, trimming each one.
pub fn parse_list<T: FromStr>(s: &str) -> Result<Vec<T>, ParseListError<T::Err>> {{
    if s.trim().is_empty() {{
        return Err(ParseListError::Empty);
    }}
    s.split(',')
        .enumerate()
        .map(|(index, item)| item.trim().parse().map_err(|source| ParseListError::Bad {{ index: {index}, source }}))
        .collect()
}}
"""


CSV_TYPES = """
/// A FromStr type whose error is a plain String (not an Error).
#[derive(Debug, PartialEq)]
struct Even(u32);

impl std::str::FromStr for Even {
    type Err = String;
    fn from_str(s: &str) -> Result<Even, String> {
        match s.parse::<u32>() {
            Ok(n) if n % 2 == 0 => Ok(Even(n)),
            _ => Err(format!("{s:?} is not even")),
        }
    }
}
"""

P.append(fix(
    "fix-generic-parse-list", "Fix: make parse_list generic", "easy", "generic-code", ["generic enums", "turbofish", "associated types", "Error"],
    """
        `parse_list` only parses `i32`. The tests call `parse_list::<T>(s)` for any `T: FromStr`, and expect
        `ParseListError<E>` to carry `T`'s own error type: `ParseListError<ParseIntError>`,
        `ParseListError<ParseBoolError>`, `ParseListError<String>`, and so on.

        Keep `Display` working whenever `E` is `Display`, and `Error` (with `source()`) whenever `E` is an `Error`, so
        `?` still converts it into `Box<dyn Error>`.
    """,
    parse_list(generic=False),
    parse_list(),
    [CSV_TYPES,
     T("integers", 'parse_list::<i32>("1, 2,3")', 'parse_list::<i32>("1, 2,3")', "Ok(vec![1, 2, 3])"),
     T("floats", 'parse_list::<f64>(" 0.5 ,-2")', 'parse_list::<f64>(" 0.5 ,-2")', "Ok(vec![0.5, -2.0])"),
     T("bad_bool", 'parse_list::<bool>("true, yes")', 'parse_list::<bool>("true, yes")', 'Err(ParseListError::Bad { index: 1, source: "yes".parse::<bool>().unwrap_err() })'),
     T("empty", 'parse_list::<u8>("  ")', 'parse_list::<u8>("  ")', "Err(ParseListError::Empty)"),
     T("error_that_is_not_an_error", 'parse_list::<Even>("2, 3"): E = String', 'parse_list::<Even>("2, 3").map_err(|e| e.to_string())', 'Err("item 1: \\"3\\" is not even".to_string())'),
     T("question_mark_into_box_dyn_error", 'sum parse_list::<i64>("4, x") through ?', "run().map_err(|e| (e.to_string(), e.source().map(|s| s.to_string())))",
       'Err(("item 1: invalid digit found in string".to_string(), Some("invalid digit found in string".to_string())))',
       setup='use std::error::Error;\nfn run() -> Result<i64, Box<dyn Error>> {\n    Ok(parse_list::<i64>("4, x")?.iter().sum())\n}')],
    [CSV_TYPES,
     T("single", 'parse_list::<u64>("18446744073709551615")', 'parse_list::<u64>("18446744073709551615")', "Ok(vec![u64::MAX])"),
     T("overflow", 'parse_list::<u8>("1,256")', 'parse_list::<u8>("1,256").map_err(|e| e.to_string())', 'Err("item 1: number too large to fit in target type".to_string())'),
     T("empty_item", 'parse_list::<i32>("1,,2")', 'parse_list::<i32>("1,,2").map_err(|e| e.to_string())', 'Err("item 1: cannot parse integer from empty string".to_string())'),
     T("first_bad_wins", 'parse_list::<i32>("a,b")', 'parse_list::<i32>("a,b").map_err(|e| e.to_string())', 'Err("item 0: invalid digit found in string".to_string())'),
     T("strings_never_fail", 'parse_list::<String>(" a , b ")', 'parse_list::<String>(" a , b ")', 'Ok(vec!["a".to_string(), "b".to_string()])'),
     T("chars", 'parse_list::<char>("é, x")', 'parse_list::<char>("é, x")', "Ok(vec!['é', 'x'])"),
     T("empty_string", 'parse_list::<i32>("")', 'parse_list::<i32>("")', "Err(ParseListError::Empty)"),
     T("even_ok", 'parse_list::<Even>("0,4")', 'parse_list::<Even>("0,4")', "Ok(vec![Even(0), Even(4)])"),
     T("empty_has_no_source", 'parse_list::<i32>(" ").source()', 'parse_list::<i32>(" ").unwrap_err().source().is_none()', "true", setup="use std::error::Error;"),
     T("source_is_the_item_error", 'parse_list::<std::net::Ipv4Addr>("1.2.3.4, 1.2.3")', 'parse_list::<std::net::Ipv4Addr>("1.2.3.4, 1.2.3").unwrap_err().source().map(|s| s.to_string())',
       'Some("invalid IPv4 address syntax".to_string())', setup="use std::error::Error;"),
     T("inferred_without_turbofish", 'let v: Vec<u16> = parse_list("7, 8")?', "v", "vec![7u16, 8]", setup='let v: Vec<u16> = parse_list("7, 8").unwrap();'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4503);
         for _ in 0..300 {
             let n = 1 + rng.below(4);
             let items: Vec<String> = (0..n).map(|_| if rng.below(5) == 0 { "x".to_string() } else { rng.int(-50, 50).to_string() }).collect();
             let s = items.join(" ,");
             let want = match items.iter().position(|x| x == "x") {
                 Some(i) => Err(format!("item {i}: invalid digit found in string")),
                 None => Ok(items.iter().map(|x| x.parse::<i8>().unwrap()).collect::<Vec<i8>>()),
             };
             check!(format!("s = {s:?}"), parse_list::<i8>(&s).map_err(|e| e.to_string()), want);
         }
     }
     """],
    [("rust", "Make the enum generic, `ParseListError<E>`, and return `ParseListError<T::Err>`: `T::Err` is `FromStr`'s associated error type."),
     ("rust", "Each impl gets its own bound: `impl<E: fmt::Display> fmt::Display for ParseListError<E>`, `impl<E: Error + 'static> Error for ParseListError<E>`."),
     ("edge", "`Even`'s error is a `String`, which isn't an `Error`. Don't require `T::Err: Error` on `parse_list` itself.")],
    ("""A generic error enum keeps the caller's error type instead of flattening it to a string. Syntax to remember: `pub enum ParseListError<E> { Empty, Bad { index: usize, source: E } }`; `pub fn parse_list<T: FromStr>(s: &str) -> Result<Vec<T>, ParseListError<T::Err>>`; calling with a turbofish, `parse_list::<i32>(s)`. `source()` returns `&(dyn Error + 'static)`, hence `E: Error + 'static` on that impl.""", "O(n)", "O(n)"),
    "Why does `Error::source` return `&(dyn Error + 'static)`? What would break for a borrowed error type like `E = &'a MyErr`?",
    ["Generic enums and per-impl bounds.", "Naming an associated type in a signature: `T::Err`.", "Turbofish: `parse_list::<T>(s)`."],
    related=("L5", "L8", "S1"),
    wrong=dict(
        one_based_index=parse_list(index="index + 1"),
        no_source=parse_list(source_body="{ let _ = source; None }"),
        display_hides_the_cause=parse_list(display_item='write!(f, "item {index}: bad value")'),
    ),
))


def max_by_key(bounds, body=None, doc="The item with the largest key; the first one on a tie. Calls `key` once per item."):
    body = body or """    let mut best = None;
    for item in items {
        let k = key(item);
        if best.as_ref().map_or(true, |(_, bk)| k > *bk) {
            best = Some((item, k));
        }
    }
    best.map(|(item, _)| item)"""
    return f"""
/// {doc}
pub fn max_by_key<T, K, F>(items: &[T], mut key: F) -> Option<&T>
where
{bounds}
{{
{body}
}}
"""


MBK_OK = "    F: FnMut(&T) -> &K,\n    K: Ord + ?Sized,"

PERSON = """
#[allow(dead_code)]
struct Person {
    name: String,
    age: u32,
    tags: Vec<i32>,
}

fn people() -> Vec<Person> {
    vec![
        Person { name: "ada".into(), age: 36, tags: vec![1, 5] },
        Person { name: "zed".into(), age: 31, tags: vec![1, 5, 0] },
        Person { name: "bob".into(), age: 36, tags: vec![2] },
    ]
}
"""

P.append(fix(
    "generic-max-by-key", "Generic max_by_key with borrowed keys", "medium", "generic-code", ["HRTB", "?Sized", "closures returning references"],
    """
        `max_by_key` returns the item with the largest key, the **first** one on a tie, calling `key` once per item.
        It's shaped like `Iterator::max_by_key`, so the key has to be an owned value. Callers want to compare by a
        field without cloning it: `max_by_key(&people, |p| p.name.as_str())`. Those calls don't compile.

        Change the signature so `key` returns a reference **into** the item, including to unsized data like `str`
        and `[i32]`. The body can stay as it is.
    """,
    max_by_key("    F: FnMut(&T) -> K,\n    K: Ord,"),
    max_by_key(MBK_OK),
    [PERSON,
     T("by_name", "people by name", "max_by_key(&v, |p| p.name.as_str()).map(|p| p.age)", "Some(31)", setup="let v = people();"),
     T("by_age_first_on_tie", "people by &age: ada and bob are both 36", "max_by_key(&v, |p| &p.age).map(|p| p.name.as_str())", 'Some("ada")', setup="let v = people();"),
     T("by_slice", "people by tags as &[i32] (lexicographic)", "max_by_key(&v, |p| p.tags.as_slice()).map(|p| p.name.as_str())", 'Some("bob")', setup="let v = people();"),
     T("empty", "no people", "max_by_key(&v, |p| p.name.as_str()).is_none()", "true", setup="let v: Vec<Person> = Vec::new();"),
     T("key_called_once_per_item", "count the calls over three people", "(best, calls)", '(Some("zed"), 3)',
       setup="let v = people();\nlet mut calls = 0;\nlet best = max_by_key(&v, |p| {\n    calls += 1;\n    p.name.as_str()\n}).map(|p| p.name.as_str());")],
    [PERSON,
     T("strings_by_str", '["pear", "Apple", "fig"] by as_str', 'max_by_key(&v, |s| s.as_str()).map(String::as_str)', 'Some("pear")',
       setup='let v = vec!["pear".to_string(), "Apple".to_string(), "fig".to_string()];'),
     T("tie_on_str_keys", '["b", "a", "b"] by as_str: which "b"?', "std::ptr::eq(max_by_key(&v, |s| s.as_str()).unwrap(), &v[0])", "true",
       setup='let v = vec!["b".to_string(), "a".to_string(), "b".to_string()];'),
     T("identity_on_integers", "[3, 9, 2] with |x| x", "max_by_key(&[3, 9, 2], |x| x)", "Some(&9)"),
     T("nested_field", "pairs by &pair.1", 'max_by_key(&v, |p| &p.1).map(|p| p.0)', "Some('b')", setup="let v = [('a', 1), ('b', 7), ('c', 7)];"),
     T("vec_of_vecs_by_slice", "[[1, 2], [1, 2, 0], [0, 9]] by as_slice", "max_by_key(&v, |x| x.as_slice())", "Some(&vec![1, 2, 0])",
       setup="let v = vec![vec![1, 2], vec![1, 2, 0], vec![0, 9]];"),
     T("single", "one person", "max_by_key(&v[..1], |p| p.name.as_str()).map(|p| p.age)", "Some(36)", setup="let v = people();"),
     T("unicode_str_keys", '["é", "z"] by as_str', 'max_by_key(&v, |s| s.as_str()).map(String::as_str)', 'Some("é")',
       setup='let v = vec!["é".to_string(), "z".to_string()];'),
     T("calls_on_empty", "no items: count the calls", "calls", "0",
       setup="let v: Vec<String> = Vec::new();\nlet mut calls = 0;\nlet _ = max_by_key(&v, |s| {\n    calls += 1;\n    s.as_str()\n});"),
     T("calls_with_ties", "five equal names: count the calls", "calls", "5",
       setup='let v = vec!["x".to_string(); 5];\nlet mut calls = 0;\nlet _ = max_by_key(&v, |s| {\n    calls += 1;\n    s.as_str()\n});'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4504);
         for _ in 0..300 {
             let n = rng.below(7);
             let words: Vec<String> = (0..n).map(|_| { let len = rng.below(3); rng.string(len, "ab") }).collect();
             let mut want: Option<usize> = None;
             for i in 0..n {
                 if want.map_or(true, |w| words[i] > words[w]) {
                     want = Some(i);
                 }
             }
             let mut calls = 0;
             let got = max_by_key(&words, |w| {
                 calls += 1;
                 w.as_str()
             });
             check!(format!("words = {words:?}"), (got.map(|r| r as *const String), calls), (want.map(|i| &words[i] as *const String), n));
         }
     }
     """],
    [("rust", "`F: FnMut(&T) -> K` means one `K` for every call, whatever the argument's lifetime, so `K` can't borrow from the argument. Tie the output to the input: `F: FnMut(&T) -> &K`."),
     ("rust", "Elided, `FnMut(&T) -> &K` is the higher-ranked `for<'a> FnMut(&'a T) -> &'a K`."),
     ("rust", "`K` is `str` or `[i32]` in the tests. Generic parameters are `Sized` unless you opt out: `K: Ord + ?Sized`.")],
    ("""The key closure must work for every borrow of an item, so its bound is higher-ranked: `for<'a> FnMut(&'a T) -> &'a K`, which the `Fn(&T) -> &K` sugar writes for you. `K: ?Sized` lets the key be `str` or `[T]` behind the reference. This is exactly why std's `max_by_key` can't return `p.name.as_str()`: its `B` is fixed before any item is borrowed.""", "O(n) key calls", "O(1)"),
    "Write the where clause with an explicit `for<'a>`. Why can't `Iterator::max_by_key` offer this signature?",
    ["Higher-ranked trait bounds: `for<'a> Fn(&'a T) -> &'a K`.", "`?Sized` for type parameters used only behind a reference."],
    rules=dict(lines=2),
    related=("L5", "L3", "L6"),
    wrong=dict(
        ties_go_last=max_by_key(MBK_OK, """    let mut best = None;
    for item in items {
        let k = key(item);
        if best.as_ref().map_or(true, |(_, bk)| k >= *bk) {
            best = Some((item, k));
        }
    }
    best.map(|(item, _)| item)"""),
        recomputes_best_key=max_by_key(MBK_OK, """    let mut best: Option<&T> = None;
    for item in items {
        if best.map_or(true, |b| key(item) > key(b)) {
            best = Some(item);
        }
    }
    best"""),
    ),
))

# ---------------------------------------------------------------- bounds & associated types (medium)


def report(fmt_body=None, generic=True):
    if not generic:
        return """
use std::fmt;

/// A titled list: the title, then one `- row` line per row.
pub struct Report<T> {
    title: String,
    rows: Vec<T>,
}

impl<T> Report<T> {
    pub fn new(title: &str, rows: Vec<T>) -> Self {
        Report { title: title.to_string(), rows }
    }

    pub fn rows(&self) -> &Vec<T> {
        &self.rows
    }
}

impl<T: fmt::Display> fmt::Display for Report<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.title)?;
        for row in &self.rows {
            write!(f, "\\n- {row}")?;
        }
        Ok(())
    }
}
"""
    fmt_body = fmt_body or """        write!(f, "{}", self.title)?;
        for row in &self.rows {
            write!(f, "\\n- {row}")?;
        }
        Ok(())"""
    return f"""
use std::fmt;

/// A titled list: the title, then one `- row` line per row.
pub struct Report<C> {{
    title: String,
    rows: C,
}}

impl<C> Report<C> {{
    pub fn new(title: &str, rows: C) -> Self {{
        Report {{ title: title.to_string(), rows }}
    }}

    pub fn rows(&self) -> &C {{
        &self.rows
    }}
}}

// `fmt` borrows `self` for a lifetime this impl can't name, so the bound must hold for every lifetime.
impl<C> fmt::Display for Report<C>
where
    for<'a> &'a C: IntoIterator,
    for<'a> <&'a C as IntoIterator>::Item: fmt::Display,
{{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{
{fmt_body}
    }}
}}
"""


EVENS = """
/// A collection that isn't Clone and whose by-reference items are owned u32s.
struct Evens(u32);

impl<'a> IntoIterator for &'a Evens {
    type Item = u32;
    type IntoIter = std::iter::StepBy<std::ops::Range<u32>>;
    fn into_iter(self) -> Self::IntoIter {
        (0..2 * self.0).step_by(2)
    }
}
"""

P.append(fix(
    "report-any-collection", "Fix: a Report over any collection (HRTB where clause)", "medium", "bounds-associated-types", ["where clauses", "HRTB", "IntoIterator"],
    """
        `Report` only holds a `Vec`. Make it hold any collection `C` (a `Vec`, an array, a `BTreeSet`, a `VecDeque`,
        an `Option`, or the tests' own `Evens`) and print whenever iterating `&C` yields printable items.

        Printing must not consume, clone or copy the rows: `rows()` still returns the collection afterwards, and
        `Evens` isn't `Clone`. The output format stays the same.
    """,
    report(generic=False),
    report(),
    [EVENS,
     T("vec", 'Report::new("Nums", vec![1, 2])', 'Report::new("Nums", vec![1, 2]).to_string()', '"Nums\\n- 1\\n- 2"'),
     T("empty_is_just_the_title", 'Report::new("None", Vec::<i32>::new())', 'Report::new("None", Vec::<i32>::new()).to_string()', '"None"'),
     T("btreeset_in_order", 'Report::new("Fruit", BTreeSet::from(["pear", "apple"]))', 'Report::new("Fruit", std::collections::BTreeSet::from(["pear", "apple"])).to_string()',
       '"Fruit\\n- apple\\n- pear"'),
     T("rows_still_there", "print a VecDeque report, then rows().len()", "(text, r.rows().len())", '("Q\\n- a\\n- b".to_string(), 2)',
       setup="let r = Report::new(\"Q\", std::collections::VecDeque::from(vec!['a', 'b']));\nlet text = r.to_string();"),
     T("own_collection", 'Report::new("Evens", Evens(3)): items are owned u32s', 'Report::new("Evens", Evens(3)).to_string()', '"Evens\\n- 0\\n- 2\\n- 4"')],
    [EVENS,
     T("array", 'Report::new("A", ["x", "y", "z"])', 'Report::new("A", ["x", "y", "z"]).to_string()', '"A\\n- x\\n- y\\n- z"'),
     T("option_some", 'Report::new("Maybe", Some(5))', 'Report::new("Maybe", Some(5)).to_string()', '"Maybe\\n- 5"'),
     T("option_none", 'Report::new("Maybe", None::<i32>)', 'Report::new("Maybe", None::<i32>).to_string()', '"Maybe"'),
     T("strings", 'Report::new("S", vec!["héllo".to_string()])', 'Report::new("S", vec!["héllo".to_string()]).to_string()', '"S\\n- héllo"'),
     T("nested_reports", "a Report of Reports", 'Report::new("outer", vec![Report::new("a", vec![1]), Report::new("b", vec![])]).to_string()', '"outer\\n- a\\n- 1\\n- b"'),
     T("empty_evens", 'Report::new("E", Evens(0))', 'Report::new("E", Evens(0)).to_string()', '"E"'),
     T("printed_twice", "print the same report twice", "(r.to_string(), r.to_string())", '("T\\n- 0".to_string(), "T\\n- 0".to_string())', setup='let r = Report::new("T", Evens(1));'),
     T("rows_returns_the_collection", "rows() of a BTreeSet report", "r.rows().iter().next().copied()", "Some(1)",
       setup='let r = Report::new("B", std::collections::BTreeSet::from([3, 1, 2]));\nlet _ = r.to_string();'),
     T("empty_title", 'Report::new("", vec![0])', 'Report::new("", vec![0]).to_string()', '"\\n- 0"'),
     """
     #[test]
     fn random_vs_format() {
         let mut rng = anneal_prelude::Rng::new(4505);
         for _ in 0..300 {
             let n = rng.below(5);
             let v: Vec<i32> = rng.vec(n, -99, 99);
             let mut want = String::from("R");
             for x in &v {
                 want.push_str(&format!("\\n- {x}"));
             }
             let r = Report::new("R", v.clone());
             check!(format!("rows = {v:?}"), (r.to_string(), r.rows().len()), (want, n));
         }
     }
     """],
    [("rust", "Store `rows: C`. The printing bound isn't on `C` but on `&C`: `&C: IntoIterator`, and its `Item: Display`."),
     ("rust", "Inside `fmt`, `&self.rows` has a lifetime the impl can't name. `impl<'a, C>` won't do; the bound has to hold for every lifetime: `where for<'a> &'a C: IntoIterator`."),
     ("rust", "Name the item type through the reference's impl: `for<'a> <&'a C as IntoIterator>::Item: fmt::Display`.")],
    ("""This is the classic use of a higher-ranked bound outside closures: 'iterating any borrow of `C` yields printable items'. Syntax to remember:
`impl<C> fmt::Display for Report<C> where for<'a> &'a C: IntoIterator, for<'a> <&'a C as IntoIterator>::Item: fmt::Display`.
Bounding `C: IntoIterator + Clone` would compile too, but it clones the whole collection on every print and shuts out `Evens`.""", "O(n) per print", "O(1)"),
    "Why doesn't `impl<'a, C> Display for Report<C> where &'a C: IntoIterator` work, even though a free function `fn show<'a, C>(c: &'a C)` could use that bound?",
    ["Higher-ranked bounds on a borrowed collection: `for<'a> &'a C: IntoIterator`.", "`where` clauses on projections: `<&'a C as IntoIterator>::Item`."],
    rules=dict(methods=["clone", "cloned", "to_vec", "to_owned"]),
    related=("L5", "L3", "S6"),
    wrong=dict(
        trailing_newline=report(fmt_body="""        writeln!(f, "{}", self.title)?;
        for row in &self.rows {
            writeln!(f, "- {row}")?;
        }
        Ok(())"""),
        comma_separated=report(fmt_body="""        write!(f, "{}", self.title)?;
        for (i, row) in (&self.rows).into_iter().enumerate() {
            write!(f, "{}{row}", if i == 0 { "\\n- " } else { ", " })?;
        }
        Ok(())"""),
    ),
))


def describe(fixed=True, none='"-"', sep='", "'):
    head = """
use std::fmt;

pub trait Describe {
    fn describe(&self) -> String;
}

/// A user name, shown in lowercase.
pub struct Name(pub String);

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.to_lowercase())
    }
}
"""
    if fixed:
        middle = """
// A blanket `impl<T: Display> Describe for T` would overlap the Vec and Option impls below (std may add
// `Display for Vec<_>` one day) and would stop other crates writing their own Describe for Display types.
macro_rules! describe_via_display {
    ($($t:ty),* $(,)?) => {
        $(
            impl Describe for $t {
                fn describe(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}

describe_via_display!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize, f32, f64, bool, char, str, String, Name);

impl<T: Describe + ?Sized> Describe for &T {
    fn describe(&self) -> String {
        (**self).describe()
    }
}
"""
    else:
        middle = """
/// Anything printable describes itself with its Display text.
impl<T: fmt::Display> Describe for T {
    fn describe(&self) -> String {
        self.to_string()
    }
}
"""
    return head + middle + f"""
/// `[a, b, c]`
impl<T: Describe> Describe for Vec<T> {{
    fn describe(&self) -> String {{
        let parts: Vec<String> = self.iter().map(|x| x.describe()).collect();
        format!("[{{}}]", parts.join({sep}))
    }}
}}

/// The value, or `-` for None.
impl<T: Describe> Describe for Option<T> {{
    fn describe(&self) -> String {{
        match self {{
            Some(x) => x.describe(),
            None => {none}.to_string(),
        }}
    }}
}}
"""


CELSIUS = """
/// A type from "another crate" whose Describe differs from its Display.
struct Celsius(f64);

impl std::fmt::Display for Celsius {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Describe for Celsius {
    fn describe(&self) -> String {
        format!("{}°C", self.0)
    }
}
"""

P.append(fix(
    "fix-conflicting-impls", "Fix: conflicting implementations (E0119)", "medium", "bounds-associated-types", ["E0119", "coherence", "blanket impls", "?Sized"],
    """
        `describe()` should work on the integer and float types, `bool`, `char`, `str`, `String`, `Name`, references to
        any of these, and `Vec<T>` / `Option<T>` of anything describable (nested too). Other crates must be able to
        implement `Describe` for their own types, even ones that already implement `Display`; the tests do.

        It doesn't compile. Keep every output format as it is.
    """,
    describe(fixed=False),
    describe(),
    [CELSIUS,
     T("scalars", "42i32, 2.5f64, true, 'x'", "(42i32.describe(), 2.5f64.describe(), true.describe(), 'x'.describe())",
       '("42".to_string(), "2.5".to_string(), "true".to_string(), "x".to_string())'),
     T("strings_and_name", '"hi", String "yo", Name("ADA")', '("hi".describe(), String::from("yo").describe(), Name("ADA".into()).describe())',
       '("hi".to_string(), "yo".to_string(), "ada".to_string())'),
     T("vec_and_option", "vec![1, 2], Some(3), None", "(vec![1, 2].describe(), Some(3).describe(), None::<i32>.describe())",
       '("[1, 2]".to_string(), "3".to_string(), "-".to_string())'),
     T("nested_with_refs", 'vec![Some("a"), None]', 'vec![Some("a"), None].describe()', '"[a, -]"'),
     T("your_own_impl", "Celsius(21.5): Display is 21.5, Describe is 21.5°C", "(Celsius(21.5).to_string(), vec![Celsius(21.5)].describe())",
       '("21.5".to_string(), "[21.5°C]".to_string())')],
    [CELSIUS,
     T("empty_vec", "Vec::<u8>::new()", "Vec::<u8>::new().describe()", '"[]"'),
     T("nested_vecs", "vec![vec![1], vec![], vec![2, 3]]", "vec![vec![1], vec![], vec![2, 3]].describe()", '"[[1], [], [2, 3]]"'),
     T("option_of_vec", "Some(vec![None, Some(1)])", "Some(vec![None, Some(1u64)]).describe()", '"[-, 1]"'),
     T("every_integer_width", "i8, i16, i64, i128, isize, u8, u16, u32, u128, usize", "vec![(-1i8).describe(), 2i16.describe(), (-3i64).describe(), 4i128.describe(), 5isize.describe(), 6u8.describe(), 7u16.describe(), 8u32.describe(), 9u128.describe(), 10usize.describe()].join(\" \")",
       '"-1 2 -3 4 5 6 7 8 9 10"'),
     T("f32", "0.5f32", "0.5f32.describe()", '"0.5"'),
     T("double_reference", '&&"x"', '(&&"x").describe()', '"x"'),
     T("vec_of_names", 'vec![Name("Bo"), Name("CY")]', 'vec![Name("Bo".into()), Name("CY".into())].describe()', '"[bo, cy]"'),
     T("option_of_celsius", "Some(Celsius(-4.0)), None", "(Some(Celsius(-4.0)).describe(), None::<Celsius>.describe())", '("-4°C".to_string(), "-".to_string())'),
     T("vec_of_strs_unicode", 'vec!["é", "ß"]', 'vec!["é", "ß"].describe()', '"[é, ß]"'),
     T("through_generic_fn", "a generic fn that takes T: Describe + ?Sized", 'show("str slice")', '"<str slice>"',
       setup="fn show<T: Describe + ?Sized>(x: &T) -> String {\n    format!(\"<{}>\", x.describe())\n}"),
     """
     #[test]
     fn random_vs_format() {
         let mut rng = anneal_prelude::Rng::new(4506);
         for _ in 0..300 {
             let n = rng.below(5);
             let v: Vec<Option<i64>> = (0..n).map(|_| if rng.bool() { Some(rng.int(-99, 99)) } else { None }).collect();
             let want = format!("[{}]", v.iter().map(|x| x.map_or("-".to_string(), |y| y.to_string())).collect::<Vec<_>>().join(", "));
             check!(format!("{v:?}"), v.describe(), want);
         }
     }
     """],
    [("rust", "Read the note on E0119: std may add `impl Display for Vec<_>` in a future version, and then your blanket impl and your `Vec` impl would both apply. Coherence rejects that now."),
     ("approach", "Drop the blanket impl. Implement `Describe` per concrete type instead: a small `macro_rules!` over a list of types avoids writing it out twenty times."),
     ("rust", "`\"a\"` inside a `Vec` is a `&str`, so add `impl<T: Describe + ?Sized> Describe for &T` (`?Sized` so `T` can be `str`).")],
    ("""A blanket impl claims every type that meets its bound, including types in other crates and types std might make `Display` later. Coherence must prove no two impls can ever overlap, so `impl<T: Display> Describe for T` rules out `impl<T> Describe for Vec<T>` and any downstream `impl Describe for Celsius`. Concrete impls (often macro-generated, as std does for integers) keep the trait open. Syntax to remember: `impl<T: Describe + ?Sized> Describe for &T`.""", "O(n) in the output size", "O(n)"),
    "Why does `impl<T: Display> Describe for T` conflict with `impl<T: Describe> Describe for Vec<T>` even though `Vec` isn't `Display` today?",
    ["Coherence and overlap: blanket impls vs specific impls (E0119).", "Macro-generated impls for a list of concrete types.", "`?Sized` on a forwarding impl for `&T`."],
    related=("L5", "L4", "L10"),
    wrong=dict(
        none_prints_none=describe(none='"None"'),
        no_space_after_comma=describe(sep='","'),
    ),
))

GRAPH_HEAD = """
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;

pub trait Graph {
    /// Each graph has one node type, so it's an associated type, not a parameter.
    type Node;

    fn neighbors(&self, node: &Self::Node) -> Vec<Self::Node>;
}

/// Nodes are 0..edges.len(); `edges[i]` lists the nodes `i` has an edge to. Edges are one-way.
pub struct AdjList {
    pub edges: Vec<Vec<usize>>,
}

/// Cells are `.` (open) or `#` (wall). Nodes are the (row, col) of open cells; moves go up, down, left or right.
pub struct Grid {
    cells: Vec<Vec<u8>>,
}
"""

GRAPH_STARTER = GRAPH_HEAD + """
impl Graph for AdjList {
    type Node = usize;

    fn neighbors(&self, node: &usize) -> Vec<usize> {
        todo!()
    }
}

impl Graph for Grid {
    type Node = (usize, usize);

    fn neighbors(&self, node: &(usize, usize)) -> Vec<(usize, usize)> {
        todo!()
    }
}

/// Rows separated by newlines.
impl From<&str> for Grid {
    fn from(text: &str) -> Self {
        todo!()
    }
}

/// One string per row.
impl From<Vec<&str>> for Grid {
    fn from(rows: Vec<&str>) -> Self {
        todo!()
    }
}

/// How many nodes can be reached from `start`, counting `start` itself.
pub fn reachable<G>(graph: &G, start: G::Node) -> usize
where
    G: Graph,
    G::Node: Eq + Hash + Clone,
{
    todo!()
}

/// The nodes on a shortest path from `from` to `to`, both included. None if `to` can't be reached.
pub fn shortest_path<G>(graph: &G, from: G::Node, to: G::Node) -> Option<Vec<G::Node>>
where
    G: Graph,
    G::Node: Eq + Hash + Clone,
{
    todo!()
}
"""


def graph_solution(grid_moves="[(0, 1), (1, 0), (0, -1), (-1, 0)]", visited="HashSet<G::Node> = HashSet::new()", count="seen.len()", adj="self.edges[*node].clone()", pop="queue.pop_front()"):
    return GRAPH_HEAD + f"""
impl Graph for AdjList {{
    type Node = usize;

    fn neighbors(&self, node: &usize) -> Vec<usize> {{
        {adj}
    }}
}}

impl Graph for Grid {{
    type Node = (usize, usize);

    fn neighbors(&self, node: &(usize, usize)) -> Vec<(usize, usize)> {{
        let (r, c) = *node;
        let mut out = Vec::new();
        for (dr, dc) in {grid_moves} {{
            let (nr, nc) = (r as isize + dr, c as isize + dc);
            if nr < 0 || nc < 0 {{
                continue;
            }}
            let (nr, nc) = (nr as usize, nc as usize);
            if self.cells.get(nr).and_then(|row| row.get(nc)) == Some(&b'.') {{
                out.push((nr, nc));
            }}
        }}
        out
    }}
}}

/// Rows separated by newlines.
impl From<&str> for Grid {{
    fn from(text: &str) -> Self {{
        Grid::from(text.lines().collect::<Vec<_>>())
    }}
}}

/// One string per row.
impl From<Vec<&str>> for Grid {{
    fn from(rows: Vec<&str>) -> Self {{
        Grid {{ cells: rows.iter().map(|r| r.as_bytes().to_vec()).collect() }}
    }}
}}

/// How many nodes can be reached from `start`, counting `start` itself.
pub fn reachable<G>(graph: &G, start: G::Node) -> usize
where
    G: Graph,
    G::Node: Eq + Hash + Clone,
{{
    let mut seen: {visited};
    seen.insert(start.clone());
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {{
        for next in graph.neighbors(&node) {{
            if !seen.contains(&next) {{
                seen.insert(next.clone());
                stack.push(next);
            }}
        }}
    }}
    {count}
}}

/// The nodes on a shortest path from `from` to `to`, both included. None if `to` can't be reached.
pub fn shortest_path<G>(graph: &G, from: G::Node, to: G::Node) -> Option<Vec<G::Node>>
where
    G: Graph,
    G::Node: Eq + Hash + Clone,
{{
    // BFS; `parent` doubles as the seen set. The start maps to itself.
    let mut parent: HashMap<G::Node, G::Node> = HashMap::new();
    parent.insert(from.clone(), from.clone());
    let mut queue = VecDeque::from([from.clone()]);
    while let Some(node) = {pop} {{
        if node == to {{
            let mut path = vec![node];
            while *path.last().unwrap() != from {{
                let prev = parent[path.last().unwrap()].clone();
                path.push(prev);
            }}
            path.reverse();
            return Some(path);
        }}
        for next in graph.neighbors(&node) {{
            if !parent.contains_key(&next) {{
                parent.insert(next.clone(), node.clone());
                queue.push_back(next);
            }}
        }}
    }}
    None
}}
"""


GRAPH_SOLUTION = graph_solution()

LADDER = """
/// A word ladder: nodes are borrowed words, edges join words that differ in one letter.
struct Ladder<'w> {
    words: &'w [&'w str],
}

impl<'w> Graph for Ladder<'w> {
    type Node = &'w str;
    fn neighbors(&self, w: &&'w str) -> Vec<&'w str> {
        self.words
            .iter()
            .copied()
            .filter(|x| x.len() == w.len() && x.chars().zip(w.chars()).filter(|(a, b)| a != b).count() == 1)
            .collect()
    }
}

const WORDS: [&str; 10] = ["cold", "cord", "card", "ward", "warm", "worm", "word", "wore", "core", "bore"];
"""


P.append(write(
    "associated-type-vs-parameter", "Associated type vs type parameter", "medium", "bounds-associated-types", ["associated types", "From", "graph traversal"],
    """
        `Graph` has an **associated type** `Node`: a graph has exactly one node type, so callers never have to name
        it. `From<T>` has a **type parameter**: `Grid` implements it twice, once for `&str` and once for `Vec<&str>`.

        Implement both graphs and both conversions, then two functions that work on *any* `Graph` whose nodes are
        `Eq + Hash + Clone` (the tests define graphs of their own, including one whose nodes are borrowed `&str`s):

        - `reachable` counts the nodes reachable from `start`, including `start`;
        - `shortest_path` returns the nodes on a shortest path, both ends included (any shortest path will do).

        - `AdjList`: `edges[i]` lists the nodes `i` has a one-way edge to.
        - `Grid`: `.` is open, `#` is a wall. From an open cell you can move up, down, left or right onto an open
          cell. Rows may have different lengths; `start` is always an open cell.
    """,
    GRAPH_STARTER,
    GRAPH_SOLUTION,
    [T("shortest_in_grid", '"...\\n.#.\\n..." from (0, 0) to (2, 2): length and ends', 'shortest_path(&Grid::from("...\\n.#.\\n..."), (0, 0), (2, 2)).map(|p| (p.len(), p[0], p[p.len() - 1]))',
       "Some((5, (0, 0), (2, 2)))"),
     T("unreachable_is_none", "edges [[1], [2], [], [0]], from 2 to 0", "shortest_path(&AdjList { edges: vec![vec![1], vec![2], vec![], vec![0]] }, 2, 0)", "None"),
     T("adj_list", "edges [[1], [2], [], [0]], start 0", "reachable(&AdjList { edges: vec![vec![1], vec![2], vec![], vec![0]] }, 0)", "3"),
     T("edges_are_one_way", "edges [[1], [2], [], [0]], start 2", "reachable(&AdjList { edges: vec![vec![1], vec![2], vec![], vec![0]] }, 2)", "1"),
     T("grid_from_str", '"..#\\n#..\\n##." from (0, 0)', 'reachable(&Grid::from("..#\\n#..\\n##."), (0, 0))', "5"),
     T("grid_from_rows", '["..", "#."] from (1, 1)', 'reachable(&Grid::from(vec!["..", "#."]), (1, 1))', "3"),
     T("your_graph_too", "a Collatz graph defined in the test, start 6", "reachable(&Collatz, 6)", "9",
       setup="struct Collatz;\nimpl Graph for Collatz {\n    type Node = u64;\n    fn neighbors(&self, n: &u64) -> Vec<u64> {\n        match *n {\n            1 => vec![],\n            n if n % 2 == 0 => vec![n / 2],\n            n => vec![3 * n + 1],\n        }\n    }\n}")],
    [LADDER,
     T("path_to_itself", "from 1 to 1", "shortest_path(&AdjList { edges: vec![vec![0], vec![0]] }, 1, 1)", "Some(vec![1])"),
     T("unique_path", "edges [[1], [2], [0, 3], []], from 0 to 3", "shortest_path(&AdjList { edges: vec![vec![1], vec![2], vec![0, 3], vec![]] }, 0, 3)", "Some(vec![0, 1, 2, 3])"),
     T("borrowed_nodes", "word ladder cold -> warm", 'shortest_path(&Ladder { words: &WORDS }, "cold", "warm").map(|p| (p.len(), p[0], p[p.len() - 1]))',
       'Some((5, "cold", "warm"))'),
     T("borrowed_nodes_reachable", "word ladder from bore", 'reachable(&Ladder { words: &WORDS }, "bore")', "10"),
     T("single_node", "edges [[]], start 0", "reachable(&AdjList { edges: vec![vec![]] }, 0)", "1"),
     T("self_loop_and_duplicates", "edges [[0, 1, 1], [0]]", "reachable(&AdjList { edges: vec![vec![0, 1, 1], vec![0]] }, 0)", "2"),
     T("cycle", "edges [[1], [2], [0]], start 1", "reachable(&AdjList { edges: vec![vec![1], vec![2], vec![0]] }, 1)", "3"),
     T("walled_in", '"###\\n#.#\\n###" from (1, 1)', 'reachable(&Grid::from("###\\n#.#\\n###"), (1, 1))', "1"),
     T("no_diagonal_moves", '".#\\n#." from (0, 0)', 'reachable(&Grid::from(".#\\n#."), (0, 0))', "1"),
     T("ragged_rows", '["...", ".", "..."] from (0, 2)', 'reachable(&Grid::from(vec!["...", ".", "..."]), (0, 2))', "7"),
     T("grid_neighbors", '"...\\n...\\n..." neighbors of (0, 0) and (1, 1)', "(g.neighbors(&(0, 0)).len(), g.neighbors(&(1, 1)).len())", "(2, 4)",
       setup='let g = Grid::from("...\\n...\\n...");'),
     T("adj_neighbors", "edges [[2, 1]] neighbors of 0", "AdjList { edges: vec![vec![2, 1], vec![], vec![]] }.neighbors(&0)", "vec![2, 1]"),
     T("node_without_copy_or_ord", "a graph whose nodes are Strings in a newtype with no Copy/Ord/Debug", 'reachable(&Words, Name("a".to_string()))', "3",
       setup='#[derive(Clone, PartialEq, Eq, Hash)]\nstruct Name(String);\nstruct Words;\nimpl Graph for Words {\n    type Node = Name;\n    fn neighbors(&self, n: &Name) -> Vec<Name> {\n        if n.0.len() < 3 { vec![Name(format!("{}a", n.0))] } else { vec![] }\n    }\n}'),
     """
     fn closure_count(edges: &[Vec<usize>], start: usize) -> usize {
         let n = edges.len();
         let mut reach = vec![vec![false; n]; n];
         for i in 0..n {
             reach[i][i] = true;
             for &j in &edges[i] {
                 reach[i][j] = true;
             }
         }
         for k in 0..n {
             for i in 0..n {
                 for j in 0..n {
                     if reach[i][k] && reach[k][j] {
                         reach[i][j] = true;
                     }
                 }
             }
         }
         reach[start].iter().filter(|&&b| b).count()
     }

     fn distance(edges: &[Vec<usize>], from: usize, to: usize) -> Option<usize> {
         let n = edges.len();
         let mut dist = vec![usize::MAX; n];
         dist[from] = 0;
         for _ in 0..n {
             for u in 0..n {
                 if dist[u] == usize::MAX {
                     continue;
                 }
                 for &v in &edges[u] {
                     dist[v] = dist[v].min(dist[u] + 1);
                 }
             }
         }
         (dist[to] != usize::MAX).then_some(dist[to])
     }

     fn flood(rows: &[String], r: usize, c: usize, seen: &mut Vec<Vec<bool>>) {
         if seen[r][c] {
             return;
         }
         seen[r][c] = true;
         let open = |r: usize, c: usize| rows.get(r).and_then(|row| row.as_bytes().get(c)) == Some(&b'.');
         if r > 0 && open(r - 1, c) {
             flood(rows, r - 1, c, seen);
         }
         if open(r + 1, c) {
             flood(rows, r + 1, c, seen);
         }
         if c > 0 && open(r, c - 1) {
             flood(rows, r, c - 1, seen);
         }
         if open(r, c + 1) {
             flood(rows, r, c + 1, seen);
         }
     }

     #[test]
     fn random_adj_lists_vs_closure() {
         let mut rng = anneal_prelude::Rng::new(4507);
         for _ in 0..300 {
             let n = 1 + rng.below(7);
             let mut edges = Vec::new();
             for _ in 0..n {
                 let k = rng.below(3);
                 let list: Vec<usize> = rng.vec(k, 0, n as i64 - 1);
                 edges.push(list);
             }
             let start = rng.below(n);
             check!(format!("edges = {edges:?}, start = {start}"), reachable(&AdjList { edges: edges.clone() }, start), closure_count(&edges, start));
             let to = rng.below(n);
             let got = shortest_path(&AdjList { edges: edges.clone() }, start, to);
             let valid = got.as_ref().map(|p| p[0] == start && p[p.len() - 1] == to && p.windows(2).all(|w| edges[w[0]].contains(&w[1])));
             check!(format!("edges = {edges:?}, from {start} to {to}"), (got.as_ref().map(|p| p.len() - 1), valid), (distance(&edges, start, to), valid.map(|_| true)));
         }
     }

     #[test]
     fn random_grids_vs_flood_fill() {
         let mut rng = anneal_prelude::Rng::new(4508);
         for _ in 0..300 {
             let h = 1 + rng.below(5);
             let rows: Vec<String> = (0..h).map(|_| { let w = 1 + rng.below(5); rng.string(w, "..#") }).collect();
             let r = rng.below(h);
             let c = rng.below(rows[r].len());
             let mut row = rows[r].clone().into_bytes();
             row[c] = b'.';
             let mut rows = rows;
             rows[r] = String::from_utf8(row).unwrap();
             let mut seen: Vec<Vec<bool>> = rows.iter().map(|row| vec![false; row.len() + 1]).collect();
             seen.push(Vec::new());
             flood(&rows, r, c, &mut seen);
             let want = seen.iter().flatten().filter(|&&b| b).count();
             let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
             check!(format!("rows = {rows:?}, start = ({r}, {c})"), reachable(&Grid::from(refs), (r, c)), want);
         }
     }

     #[test]
     fn scale_open_grid() {
         let row = ".".repeat(600);
         let rows: Vec<&str> = (0..600).map(|_| row.as_str()).collect();
         let g = Grid::from(rows);
         check!("600 x 600 open grid from the corner", reachable(&g, (599, 599)), 360_000);
         check!("600 x 600 open grid, corner to corner", shortest_path(&g, (0, 0), (599, 599)).map(|p| p.len()), Some(1199));
     }

     #[test]
     fn scale_long_chain() {
         let n = 200_000;
         let edges: Vec<Vec<usize>> = (0..n).map(|i| if i + 1 < n { vec![i + 1] } else { vec![] }).collect();
         check!("a chain of 200000 nodes", reachable(&AdjList { edges }, 0), 200_000);
     }
     """],
    [("approach", "`reachable`: DFS with a `HashSet<G::Node>`. `shortest_path`: BFS with a `VecDeque` and a `HashMap<G::Node, G::Node>` of parents, which also marks nodes as seen; walk the parents back from `to`."),
     ("rust", "`G::Node` is the graph's own node type. Because it's associated, `reachable<G>` needs no second type parameter, and `reachable(&grid, (0, 0))` needs no annotation."),
     ("edge", "In `Grid::neighbors`, compute moves in `isize` (or check `r > 0`) so row 0 doesn't underflow, and use `.get()` for ragged rows.")],
    ("""Syntax to remember: `trait Graph { type Node; fn neighbors(&self, node: &Self::Node) -> Vec<Self::Node>; }`, `impl Graph for Grid { type Node = (usize, usize); ... }`, and in generic code `G::Node` (or `<G as Graph>::Node`). Associated type: one implementation per type, chosen by the implementor (`Iterator::Item`, `Deref::Target`). Type parameter: many implementations per type, chosen by the caller (`From<T>`, `Add<Rhs>`). With `trait Graph<N>`, a type could be a graph over several node types and every caller would have to say which.""", "O(V + E)", "O(V)"),
    "Rewrite `Graph` as `trait Graph<N>`. What changes in `reachable`'s signature, and when would that design be the right one?",
    ["Associated types: one impl per type, picked by the implementor.", "Type parameters: many impls per type, picked by the caller (`From<T>`)."],
    related=("L5", "L4", "D9"),
    wrong=dict(
        diagonal_moves=graph_solution(grid_moves="[(0, 1), (1, 0), (0, -1), (-1, 0), (1, 1), (-1, -1), (1, -1), (-1, 1)]"),
        vec_as_visited_set=graph_solution(visited="Vec<G::Node> = Vec::new();\n    let mut seen = VecSet(seen)").replace(
            "/// How many nodes", "struct VecSet<T>(Vec<T>);\n\nimpl<T: PartialEq> VecSet<T> {\n    fn insert(&mut self, x: T) {\n        self.0.push(x);\n    }\n\n    fn contains(&self, x: &T) -> bool {\n        self.0.contains(x)\n    }\n\n    fn len(&self) -> usize {\n        self.0.len()\n    }\n}\n\n/// How many nodes"),
        dfs_path=graph_solution(pop="queue.pop_back()"),
        undirected_edges=graph_solution(adj="let mut out = self.edges[*node].clone();\n        for (i, list) in self.edges.iter().enumerate() {\n            if list.contains(node) {\n                out.push(i);\n            }\n        }\n        out"),
    ),
))

BUILDER_HEAD = """
use std::marker::PhantomData;

#[derive(Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,
}

#[derive(Debug, PartialEq)]
pub enum UrlError {
    Empty,
    NoScheme,
}

/// The builder's states: before and after it has a URL.
pub struct NoUrl;
pub struct HasUrl;

pub struct RequestBuilder<S> {
    url: String,
    headers: Vec<(String, String)>,
    timeout_ms: u64,
    state: PhantomData<S>,
}
"""

BUILDER_STARTER = BUILDER_HEAD + """
impl RequestBuilder<NoUrl> {
    /// No URL, no headers, a 30 000 ms timeout.
    pub fn new() -> Self {
        todo!()
    }

    /// The only way to get a builder that can `build`.
    pub fn url(self, url: &str) -> Result<RequestBuilder<HasUrl>, UrlError> {
        todo!()
    }
}

impl<S> RequestBuilder<S> {
    pub fn header(self, key: &str, value: &str) -> Self {
        todo!()
    }

    pub fn timeout_ms(self, ms: u64) -> Self {
        todo!()
    }
}

impl RequestBuilder<HasUrl> {
    pub fn build(self) -> Request {
        todo!()
    }
}
"""


def builder_solution(new_headers="self.headers", header_body="self.headers.push((key.to_string(), value.to_string()));", scheme='url.starts_with("http://") || url.starts_with("https://")'):
    return BUILDER_HEAD + f"""
impl RequestBuilder<NoUrl> {{
    /// No URL, no headers, a 30 000 ms timeout.
    pub fn new() -> Self {{
        RequestBuilder {{ url: String::new(), headers: Vec::new(), timeout_ms: 30_000, state: PhantomData }}
    }}

    /// The only way to get a builder that can `build`.
    pub fn url(self, url: &str) -> Result<RequestBuilder<HasUrl>, UrlError> {{
        let url = url.trim();
        if url.is_empty() {{
            return Err(UrlError::Empty);
        }}
        if !({scheme}) {{
            return Err(UrlError::NoScheme);
        }}
        // A different type parameter means a new value: move each field across.
        Ok(RequestBuilder {{ url: url.to_string(), headers: {new_headers}, timeout_ms: self.timeout_ms, state: PhantomData }})
    }}
}}

impl<S> RequestBuilder<S> {{
    pub fn header(mut self, key: &str, value: &str) -> Self {{
        {header_body}
        self
    }}

    pub fn timeout_ms(mut self, ms: u64) -> Self {{
        self.timeout_ms = ms;
        self
    }}
}}

impl RequestBuilder<HasUrl> {{
    pub fn build(self) -> Request {{
        Request {{ url: self.url, headers: self.headers, timeout_ms: self.timeout_ms }}
    }}
}}
"""


def req(url, headers=(), timeout=30000):
    hs = ", ".join(f'("{k}".to_string(), "{v}".to_string())' for k, v in headers)
    return f'Request {{ url: "{url}".to_string(), headers: vec![{hs}], timeout_ms: {timeout} }}'


P.append(write(
    "typestate-http-builder", "Typestate HTTP builder", "medium", "bounds-associated-types", ["typestate", "builder", "PhantomData"],
    """
        Build HTTP requests with a builder whose **type** says whether it has a URL yet. `RequestBuilder<NoUrl>`
        has no `build` method, so "forgot the URL" is a compile error instead of a runtime one.

        - `new()` starts with no headers and a 30 000 ms timeout.
        - `url(u)` trims `u`. It fails with `UrlError::Empty` if nothing is left, and with `UrlError::NoScheme` unless
          it starts with `http://` or `https://`. On success it moves everything set so far into a
          `RequestBuilder<HasUrl>`.
        - `header` and `timeout_ms` work in either state. Headers accumulate in call order (a repeated key is
          kept twice); the last `timeout_ms` wins.
        - `build()` exists only on `RequestBuilder<HasUrl>` and can't fail.
    """,
    BUILDER_STARTER,
    builder_solution(),
    [T("minimal", 'new().url("https://a.io")?.build()', 'RequestBuilder::new().url("https://a.io").unwrap().build()', req("https://a.io")),
     T("full_chain", "headers and a timeout after the URL", 'b.header("Accept", "json").timeout_ms(500).header("X-Id", "7").build()',
       req("https://a.io/x", [("Accept", "json"), ("X-Id", "7")], 500),
       setup='let b: RequestBuilder<HasUrl> = RequestBuilder::new().url("https://a.io/x").unwrap();'),
     T("settings_before_the_url_are_kept", "header and timeout before url()", 'b.url("http://b.io").unwrap().build()',
       req("http://b.io", [("K", "v")], 10),
       setup='let b: RequestBuilder<NoUrl> = RequestBuilder::new().header("K", "v").timeout_ms(10);'),
     T("empty_url", 'url("  ")', 'RequestBuilder::new().url("  ").err()', "Some(UrlError::Empty)"),
     T("no_scheme", 'url("a.io")', 'RequestBuilder::new().url("a.io").err()', "Some(UrlError::NoScheme)")],
    [T("url_is_trimmed", 'url("  https://t.io \\n")', 'RequestBuilder::new().url("  https://t.io \\n").unwrap().build().url', '"https://t.io"'),
     T("empty_string", 'url("")', 'RequestBuilder::new().url("").err()', "Some(UrlError::Empty)"),
     T("ftp_is_rejected", 'url("ftp://x")', 'RequestBuilder::new().url("ftp://x").err()', "Some(UrlError::NoScheme)"),
     T("scheme_is_case_sensitive", 'url("HTTPS://x")', 'RequestBuilder::new().url("HTTPS://x").err()', "Some(UrlError::NoScheme)"),
     T("scheme_must_be_at_the_start", 'url("see https://x")', 'RequestBuilder::new().url("see https://x").err()', "Some(UrlError::NoScheme)"),
     T("repeated_header_kept_twice", 'header("A", "1").header("A", "2")', 'RequestBuilder::new().header("A", "1").url("https://x").unwrap().header("A", "2").build().headers',
       'vec![("A".to_string(), "1".to_string()), ("A".to_string(), "2".to_string())]'),
     T("last_timeout_wins", "timeout_ms(1) before url, timeout_ms(2) after", 'RequestBuilder::new().timeout_ms(1).url("https://x").unwrap().timeout_ms(2).build().timeout_ms', "2"),
     T("zero_timeout", "timeout_ms(0)", 'RequestBuilder::new().url("https://x").unwrap().timeout_ms(0).build().timeout_ms', "0"),
     T("unicode_header", 'header("X-Name", "Zoë")', 'RequestBuilder::new().url("https://x").unwrap().header("X-Name", "Zoë").build()',
       req("https://x", [("X-Name", "Zoë")])),
     T("scheme_only", 'url("http://")', 'RequestBuilder::new().url("http://").unwrap().build().url', '"http://"'),
     """
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(4509);
         for _ in 0..300 {
             let mut log = Vec::new();
             let mut headers = Vec::new();
             let mut timeout = 30_000u64;
             let mut b = RequestBuilder::new();
             let before = rng.below(4);
             for _ in 0..before {
                 if rng.bool() {
                     let k = rng.string(1, "AB");
                     let v = rng.below(10).to_string();
                     log.push(format!("header({k:?}, {v:?})"));
                     b = b.header(&k, &v);
                     headers.push((k, v));
                 } else {
                     let t = rng.below(1000) as u64;
                     log.push(format!("timeout_ms({t})"));
                     b = b.timeout_ms(t);
                     timeout = t;
                 }
             }
             let url = format!("https://h{}.io", rng.below(10));
             log.push(format!("url({url:?})"));
             let mut b = b.url(&url).unwrap();
             let after = rng.below(4);
             for _ in 0..after {
                 if rng.bool() {
                     let k = rng.string(1, "AB");
                     let v = rng.below(10).to_string();
                     log.push(format!("header({k:?}, {v:?})"));
                     b = b.header(&k, &v);
                     headers.push((k, v));
                 } else {
                     let t = rng.below(1000) as u64;
                     log.push(format!("timeout_ms({t})"));
                     b = b.timeout_ms(t);
                     timeout = t;
                 }
             }
             check!(log.join("."), b.build(), Request { url, headers, timeout_ms: timeout });
         }
     }
     """],
    [("rust", "`PhantomData<S>` makes `S` part of the type at zero size. Split the methods into `impl RequestBuilder<NoUrl>`, `impl<S> RequestBuilder<S>` and `impl RequestBuilder<HasUrl>`."),
     ("rust", "`url` can't return `self` with a new state: `RequestBuilder<HasUrl>` is a different type. Build a new struct, moving each field across (`..self` doesn't work when the type parameter changes)."),
     ("edge", "Carry the headers and timeout across the transition; settings made before `url()` must survive.")],
    ("Typestate moves a runtime check (\"is there a URL?\") into the type system: `build` simply doesn't exist until `url` has succeeded, so `build` can return `Request` instead of `Result`. The states are zero-sized, so this costs nothing at run time.", "O(1) per call", "O(headers)"),
    "How would you require at least one `header` call before `build`, also at compile time? What does each extra state cost the API?",
    ["Typestate: marker types plus `PhantomData` pick which methods exist.", "Consuming builders (`self` in, `Self` out) and moving fields across a state change."],
    source="W11", related=("L5", "M1"),
    wrong=dict(
        drops_early_headers=builder_solution(new_headers="Vec::new()"),
        headers_overwrite=builder_solution(header_body="self.headers.retain(|(k, _)| k != key);\n        self.headers.push((key.to_string(), value.to_string()));"),
        scheme_anywhere=builder_solution(scheme='url.contains("://")'),
    ),
))


# ---------------------------------------------------------------- compile-time rust (medium)

RING_STARTER = """
/// A fixed-capacity ring buffer. Pushing onto a full ring evicts and returns the oldest item.
pub struct Ring<T, const N: usize> {
    items: [T; N],
    start: usize,
    len: usize,
}

impl<T: Default, const N: usize> Ring<T, N> {
    pub fn new() -> Self {
        Ring { items: [T::default(); N], start: 0, len: 0 }
    }
}

impl<T, const N: usize> Ring<T, N> {
    pub const CAPACITY: usize = N;

    /// Adds `x` as the newest item; if the ring was full, returns the evicted oldest one.
    pub fn push(&mut self, x: T) -> Option<T> {
        if self.len == N {
            let old = std::mem::replace(&mut self.items[self.start], x);
            self.start = (self.start + 1) % N;
            Some(old)
        } else {
            self.items[(self.start + self.len) % N] = x;
            self.len += 1;
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).map(move |i| &self.items[(self.start + i) % N])
    }
}

/// `a` followed by `b`.
pub fn concat<T: Copy + Default, const A: usize, const B: usize>(a: [T; A], b: [T; B]) -> [T; A + B] {
    let mut out = [T::default(); A + B];
    out[..A].copy_from_slice(&a);
    out[A..].copy_from_slice(&b);
    out
}
"""


def ring_solution(zero_guard="        if N == 0 {\n            return Some(x);\n        }\n", iter_index="(self.start + i) % N", concat_iter="a.into_iter().chain(b)"):
    return f"""
/// A fixed-capacity ring buffer. Pushing onto a full ring evicts and returns the oldest item.
pub struct Ring<T, const N: usize> {{
    // Option<T> slots: an empty ring needs no T at all, so no `T: Default` and no `T: Copy`.
    items: [Option<T>; N],
    start: usize,
    len: usize,
}}

impl<T, const N: usize> Ring<T, N> {{
    pub const CAPACITY: usize = N;

    pub fn new() -> Self {{
        // `[None; N]` would need `Option<T>: Copy`; from_fn builds each slot separately.
        Ring {{ items: std::array::from_fn(|_| None), start: 0, len: 0 }}
    }}

    /// Adds `x` as the newest item; if the ring was full, returns the evicted oldest one.
    pub fn push(&mut self, x: T) -> Option<T> {{
{zero_guard}        if self.len == N {{
            let old = self.items[self.start].replace(x);
            self.start = (self.start + 1) % N;
            old
        }} else {{
            self.items[(self.start + self.len) % N] = Some(x);
            self.len += 1;
            None
        }}
    }}

    pub fn len(&self) -> usize {{
        self.len
    }}

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {{
        (0..self.len).map(move |i| self.items[{iter_index}].as_ref().unwrap())
    }}
}}

/// `a` followed by `b`. `[T; A + B]` isn't allowed on stable, so the caller's length `C` is checked at compile time.
pub fn concat<T, const A: usize, const B: usize, const C: usize>(a: [T; A], b: [T; B]) -> [T; C] {{
    const {{ assert!(A + B == C, "concat: C must be A + B") }};
    let mut items = {concat_iter};
    std::array::from_fn(|_| items.next().unwrap())
}}
"""


TOKEN2 = """
/// No derives: not Default, Clone or Copy.
struct Token(u32);
"""

P.append(fix(
    "fix-const-generic-traps", "Fix: const generic traps", "medium", "compile-time-rust", ["const generics", "E0277", "array::from_fn", "inline const"],
    """
        `Ring<T, N>` is a fixed-capacity ring buffer and `concat` joins two arrays. Neither compiles, and neither
        should need anything from `T`: the tests use `String` and a `Token` type with no derives.

        - `Ring::new()` must work for any `T`, and `N` may be `0` (a zero-capacity ring hands every pushed item
          straight back).
        - `concat` returns `[T; C]`, where the caller picks `C` (usually by annotating the result) and it must equal
          `A + B`.

        Keep the storage in arrays.
    """,
    RING_STARTER,
    ring_solution(),
    [TOKEN2,
     T("ring_of_strings", "Ring<String, 2>: push a, b, c", "(pushed, r.iter().cloned().collect::<Vec<_>>())", '(vec![None, None, Some("a".to_string())], vec!["b".to_string(), "c".to_string()])',
       setup='let mut r: Ring<String, 2> = Ring::new();\nlet pushed: Vec<Option<String>> = ["a", "b", "c"].iter().map(|s| r.push(s.to_string())).collect();'),
     T("no_default_needed", "Ring<Token, 2>: push 1, 2, 3", "(evicted, r.iter().map(|t| t.0).collect::<Vec<_>>())", "(Some(1), vec![2, 3])",
       setup="let mut r: Ring<Token, 2> = Ring::new();\nr.push(Token(1));\nr.push(Token(2));\nlet evicted = r.push(Token(3)).map(|t| t.0);"),
     T("zero_capacity", "Ring<i32, 0>: push 5", "(r.push(5), r.len(), Ring::<i32, 0>::CAPACITY)", "(Some(5), 0, 0)", setup="let mut r: Ring<i32, 0> = Ring::new();"),
     T("concat_ints", "concat([1, 2], [3, 4, 5]) as [i32; 5]", "c", "[1, 2, 3, 4, 5]", setup="let c: [i32; 5] = concat([1, 2], [3, 4, 5]);"),
     T("concat_strings", 'concat(["a"], ["b", "c"]) as Strings', "c", '["a".to_string(), "b".to_string(), "c".to_string()]',
       setup='let c: [String; 3] = concat(["a".to_string()], ["b".to_string(), "c".to_string()]);')],
    [TOKEN2,
     T("capacity_is_a_const", "const CAP: usize = Ring::<u8, 4>::CAPACITY", "CAP", "4", setup="const CAP: usize = Ring::<u8, 4>::CAPACITY;"),
     T("empty_ring", "Ring<i32, 3>::new()", "(r.len(), r.iter().count())", "(0, 0)", setup="let r: Ring<i32, 3> = Ring::new();"),
     T("wraps_many_times", "Ring<u32, 3>: push 0..10", "(r.len(), r.iter().copied().collect::<Vec<_>>())", "(3, vec![7, 8, 9])",
       setup="let mut r: Ring<u32, 3> = Ring::new();\nfor i in 0..10 {\n    r.push(i);\n}"),
     T("zero_capacity_tokens", "Ring<Token, 0>: push Token(9)", "r.push(Token(9)).map(|t| t.0)", "Some(9)", setup="let mut r: Ring<Token, 0> = Ring::new();"),
     T("capacity_one", "Ring<char, 1>: push a, b", "(r.push('a'), r.push('b'), r.iter().copied().collect::<String>())", "(None, Some('a'), \"b\".to_string())",
       setup="let mut r: Ring<char, 1> = Ring::new();"),
     T("concat_empty", "concat([], []) as [u8; 0]", "c.len()", "0", setup="let c: [u8; 0] = concat([], []);"),
     T("concat_turbofish", "concat::<char, 1, 2, 3>(['a'], ['b', 'c'])", "concat::<char, 1, 2, 3>(['a'], ['b', 'c'])", "['a', 'b', 'c']"),
     T("concat_tokens", "concat([Token(1)], [Token(2)])", "c.map(|t| t.0)", "[1, 2]", setup="let c: [Token; 2] = concat([Token(1)], [Token(2)]);"),
     T("drops_evicted_and_remaining", "Rc clones through a Ring<Rc<i32>, 2>, then drop it", "std::rc::Rc::strong_count(&rc)", "1",
       setup="let rc = std::rc::Rc::new(0);\nlet mut r: Ring<std::rc::Rc<i32>, 2> = Ring::new();\nfor _ in 0..5 {\n    r.push(rc.clone());\n}\ndrop(r);"),
     """
     #[test]
     fn random_vs_vecdeque() {
         let mut rng = anneal_prelude::Rng::new(4510);
         for _ in 0..300 {
             let mut r: Ring<i64, 4> = Ring::new();
             let mut model = std::collections::VecDeque::new();
             let n = rng.below(12);
             let xs: Vec<i64> = rng.vec(n, -9, 9);
             for &x in &xs {
                 let want = if model.len() == 4 { model.pop_front() } else { None };
                 model.push_back(x);
                 check!(format!("push {xs:?} into Ring<_, 4>"), r.push(x), want);
             }
             check!(format!("push {xs:?} into Ring<_, 4>"), (r.len(), r.iter().copied().collect::<Vec<_>>()), (model.len(), model.iter().copied().collect::<Vec<_>>()));
         }
     }

     #[test]
     fn random_concat() {
         let mut rng = anneal_prelude::Rng::new(4511);
         for _ in 0..200 {
             let a: [i32; 3] = [rng.int(-9, 9) as i32, rng.int(-9, 9) as i32, rng.int(-9, 9) as i32];
             let b: [i32; 2] = [rng.int(-9, 9) as i32, rng.int(-9, 9) as i32];
             let c: [i32; 5] = concat(a, b);
             check!(format!("concat({a:?}, {b:?})"), c.to_vec(), [a.to_vec(), b.to_vec()].concat());
         }
     }
     """],
    [("rust", "`[expr; N]` copies `expr`, so it needs `Copy` (or a constant). `std::array::from_fn(|_| ...)` builds each element separately. For a ring that needs nothing from `T`, store `[Option<T>; N]`."),
     ("rust", "`[T; A + B]` is 'generic parameters may not be used in const operations': stable Rust can't compute with const parameters in types. Take a third `const C: usize` and check `A + B == C` in an inline `const { assert!(...) }` block."),
     ("edge", "With `N == 0` the ring is always full, so `push` indexes an empty array (index out of bounds), and `% N` would divide by zero. Return the item before touching the array.")],
    ("""Const generics make lengths part of the type, but stable Rust only lets a const parameter stand alone in a type (`[T; N]`), not in arithmetic (`[T; A + B]`, which needs the unstable `generic_const_exprs`). The usual workaround is an extra parameter plus a compile-time check: `const { assert!(A + B == C) }` fails the build for a bad `C` after monomorphization. Syntax to remember: `impl<T, const N: usize> Ring<T, N> { pub const CAPACITY: usize = N; }`, `std::array::from_fn(|i| ...)`, and a call with the turbofish `concat::<char, 1, 2, 3>(...)`.""", "O(1) per push; O(A + B) concat", "O(N)"),
    "`concat::<i32, 2, 2, 5>(...)` is rejected, but only when it's instantiated. Why can't the check happen at the definition, and how does `generic_const_exprs` change that?",
    ["`[x; N]` needs `Copy`; `array::from_fn` doesn't.", "No arithmetic on const parameters in types on stable; an extra parameter plus `const { assert!() }`.", "`N = 0` is a legal const argument."],
    rules=dict(types=["Vec", "VecDeque"], unsafe=True),
    related=("L5", "S3", "Y1"),
    wrong=dict(
        forgets_zero_capacity=ring_solution(zero_guard=""),
        newest_first=ring_solution(iter_index="(self.start + self.len - 1 - i) % N"),
        concat_b_first=ring_solution(concat_iter="b.into_iter().chain(a)"),
    ),
))


def crc(table_body=None, checksum_body=None):
    table_body = table_body or """    let mut table = [0u8; 256];
    let mut b = 0;
    // `for` loops call Iterator::next, which isn't const; use `while`.
    while b < 256 {
        let mut crc = b as u8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ poly } else { crc << 1 };
            bit += 1;
        }
        table[b] = crc;
        b += 1;
    }
    table"""
    checksum_body = checksum_body or """        let mut crc = 0u8;
        let mut i = 0;
        while i < data.len() {
            crc = Self::TABLE[(crc ^ data[i]) as usize];
            i += 1;
        }
        crc"""
    return f"""
/// CRC-8 with polynomial `POLY`: initial value 0, no reflection, no final XOR.
pub struct Crc8<const POLY: u8>;

/// `table[b]` is the CRC of the single byte `b`.
pub const fn make_table(poly: u8) -> [u8; 256] {{
{table_body}
}}

impl<const POLY: u8> Crc8<POLY> {{
    /// Built at compile time, once per polynomial.
    pub const TABLE: [u8; 256] = make_table(POLY);

    pub const fn checksum(data: &[u8]) -> u8 {{
{checksum_body}
    }}
}}
"""


CRC_STARTER = crc(table_body="    // TODO: build the table. (A placeholder, so the crate compiles.)\n    [0; 256]",
                  checksum_body="        // TODO: one table lookup per byte. (A placeholder, so the crate compiles.)\n        0")

P.append(write(
    "const-fn-crc-table", "const fn: a CRC table built at compile time", "medium", "compile-time-rust", ["const fn", "associated consts", "const generics"],
    """
        Implement CRC-8 for any polynomial `POLY` (initial value 0, bits processed most significant first, no final
        XOR). For each input byte: `crc ^= byte`, then 8 times shift `crc` left by one, XORing in `POLY` whenever the
        bit shifted out was 1.

        `make_table(poly)` returns `table[b]` = the CRC of the single byte `b`. `Crc8::<POLY>::TABLE` stores it,
        computed by the compiler, and `checksum` does one table lookup per byte:
        `crc = TABLE[(crc ^ byte) as usize]`. Both must be usable in `const` items; the tests call them there.
    """,
    CRC_STARTER,
    crc(),
    [T("check_value_smbus", 'const C: u8 = Crc8::<0x07>::checksum(b"123456789")', "C", "0xF4", setup='const C: u8 = Crc8::<0x07>::checksum(b"123456789");'),
     T("table_entries", "Crc8::<0x07>::TABLE[1], [2], [0x80]", "(T[1], T[2], T[0x80])", "(0x07, 0x0E, 0x89)", setup="const T: [u8; 256] = Crc8::<0x07>::TABLE;"),
     T("empty_input", "checksum(b\"\")", 'Crc8::<0x07>::checksum(b"")', "0"),
     T("another_polynomial", 'Crc8::<0x31>::checksum(b"123456789")', 'Crc8::<0x31>::checksum(b"123456789")', "0xA2"),
     T("runtime_data", "checksum of a String's bytes at run time", "Crc8::<0x07>::checksum(s.as_bytes())", "0x9E", setup='let s = String::from("héllo");')],
    [T("single_byte_is_table_entry", "checksum(b\"a\") == TABLE[b'a']", '(Crc8::<0x07>::checksum(b"a"), Crc8::<0x07>::TABLE[b\'a\' as usize])', "(0x20, 0x20)"),
     T("zero_byte", "checksum(&[0])", "Crc8::<0x07>::checksum(&[0])", "0"),
     T("table_zero_entry", "TABLE[0] for two polynomials", "(Crc8::<0x07>::TABLE[0], Crc8::<0x31>::TABLE[0])", "(0, 0)"),
     T("table_last_entry", "TABLE[0xFF] for 0x07 and 0x31", "(Crc8::<0x07>::TABLE[0xFF], Crc8::<0x31>::TABLE[0xFF])", "(0xF3, 0xAC)"),
     T("make_table_directly", "const M: [u8; 256] = make_table(0x31)", "(M[1], M[0x80])", "(0x31, 0x7A)", setup="const M: [u8; 256] = make_table(0x31);"),
     T("poly_0x1d", 'Crc8::<0x1D>::checksum(b"123456789")', 'Crc8::<0x1D>::checksum(b"123456789")', "0x37"),
     T("poly_0x9b_in_const", 'const X: u8 = Crc8::<0x9B>::checksum(b"123456789")', "X", "0xEA", setup='const X: u8 = Crc8::<0x9B>::checksum(b"123456789");'),
     T("all_bytes", "checksum of 0..=255", "Crc8::<0x07>::checksum(&bytes)", "0x14", setup="let bytes: Vec<u8> = (0..=255).collect();"),
     T("appending_the_crc_gives_zero", "data followed by its own CRC checks to 0", "Crc8::<0x07>::checksum(&data)", "0",
       setup='let mut data = b"anneal".to_vec();\ndata.push(Crc8::<0x07>::checksum(&data));'),
     """
     fn bitwise(data: &[u8], poly: u8) -> u8 {
         let mut crc = 0u8;
         for &b in data {
             crc ^= b;
             for _ in 0..8 {
                 crc = if crc & 0x80 != 0 { (crc << 1) ^ poly } else { crc << 1 };
             }
         }
         crc
     }

     #[test]
     fn random_vs_bitwise() {
         let mut rng = anneal_prelude::Rng::new(4512);
         for _ in 0..300 {
             let n = rng.below(20);
             let data: Vec<u8> = rng.vec(n, 0, 255);
             check!(format!("data = {data:?}"), (Crc8::<0x07>::checksum(&data), Crc8::<0x31>::checksum(&data), Crc8::<0xD5>::checksum(&data)),
                    (bitwise(&data, 0x07), bitwise(&data, 0x31), bitwise(&data, 0xD5)));
         }
     }

     #[test]
     fn tables_vs_bitwise() {
         const T7: [u8; 256] = Crc8::<0x07>::TABLE;
         for b in 0..=255u8 {
             check!(format!("TABLE[{b}] for 0x07"), T7[b as usize], bitwise(&[b], 0x07));
         }
     }
     """],
    [("rust", "`for` loops aren't allowed in a `const fn` (they call `Iterator::next`, which isn't const). Loop with `while` and a counter."),
     ("approach", "Table: for each byte `b`, start with `crc = b` and run the 8 shift-and-XOR steps. Checksum: `crc = TABLE[(crc ^ byte) as usize]` for each byte, starting from 0."),
     ("rust", "Inside the impl, the table is `Self::TABLE`. It's an associated const, evaluated at compile time once per `POLY` that's used.")],
    ("""A `const fn` can run at compile time, so `TABLE` costs nothing at run time and `checksum` works in `const` items. The limits are what make it 'understand' level: no `for`, no iterator adaptors, no trait method calls (like `Iterator::next` or most operators on generic types), and `while` with manual counters instead. An associated const on a const-generic type (`impl<const POLY: u8> Crc8<POLY> { pub const TABLE: [u8; 256] = make_table(POLY); }`) is evaluated per instantiation. Syntax to remember: `Crc8::<0x07>::TABLE`.""", "O(n) per checksum; the table is built at compile time", "256 bytes per polynomial, in the binary"),
    "What happens if `make_table` panics (say, an index out of bounds) when `TABLE` is evaluated? When is that error reported?",
    ["`const fn` and its limits: `while` loops, no iterators.", "Associated consts on const-generic types, evaluated per instantiation."],
    related=("L5", "Y4"),
    wrong=dict(
        init_ff=crc(checksum_body="""        let mut crc = 0xFFu8;
        let mut i = 0;
        while i < data.len() {
            crc = Self::TABLE[(crc ^ data[i]) as usize];
            i += 1;
        }
        crc"""),
        shifts_seven_times=crc(table_body="""    let mut table = [0u8; 256];
    let mut b = 0;
    while b < 256 {
        let mut crc = b as u8;
        let mut bit = 0;
        while bit < 7 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ poly } else { crc << 1 };
            bit += 1;
        }
        table[b] = crc;
        b += 1;
    }
    table"""),
        index_by_byte_only=crc(checksum_body="""        let mut crc = 0u8;
        let mut i = 0;
        while i < data.len() {
            crc ^= Self::TABLE[data[i] as usize];
            i += 1;
        }
        crc"""),
    ),
))


def sorted_vec(pos="self.items.partition_point(|y| O::cmp(y, &x) != Ordering::Greater)", contains="self.items.binary_search_by(|y| O::cmp(y, x)).is_ok()", asc="a.cmp(b)"):
    return f"""
use std::cmp::Ordering;
use std::marker::PhantomData;

/// How to order items. Implementors are zero-sized: the strategy lives in the type, not in the value.
pub trait Order {{
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering;
}}

pub struct Asc;
pub struct Desc;

impl Order for Asc {{
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {{
        {asc}
    }}
}}

impl Order for Desc {{
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {{
        b.cmp(a)
    }}
}}

/// Items kept sorted by `O`, ascending unless you say otherwise.
pub struct SortedVec<T, O = Asc> {{
    items: Vec<T>,
    order: PhantomData<O>,
}}

impl<T: Ord, O: Order> SortedVec<T, O> {{
    pub fn new() -> Self {{
        SortedVec {{ items: Vec::new(), order: PhantomData }}
    }}

    /// Inserts `x` after any items that compare equal to it.
    pub fn insert(&mut self, x: T) {{
        let i = {pos};
        self.items.insert(i, x);
    }}

    /// Whether some item compares equal to `x` under `O`.
    pub fn contains(&self, x: &T) -> bool {{
        {contains}
    }}

    pub fn as_slice(&self) -> &[T] {{
        &self.items
    }}

    /// The same items sorted by another order; equal items keep their relative order.
    pub fn reorder<O2: Order>(mut self) -> SortedVec<T, O2> {{
        self.items.sort_by(O2::cmp);
        SortedVec {{ items: self.items, order: PhantomData }}
    }}
}}

impl<T: Ord, O: Order> FromIterator<T> for SortedVec<T, O> {{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {{
        let mut v = SortedVec::new();
        for x in iter {{
            v.insert(x);
        }}
        v
    }}
}}
"""


SORTED_STARTER = """
use std::cmp::Ordering;
use std::marker::PhantomData;

/// How to order items. Implementors are zero-sized: the strategy lives in the type, not in the value.
pub trait Order {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering;
}

pub struct Asc;
pub struct Desc;

impl Order for Asc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        todo!()
    }
}

impl Order for Desc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        todo!()
    }
}

/// Items kept sorted by `O`, ascending unless you say otherwise.
pub struct SortedVec<T, O = Asc> {
    items: Vec<T>,
    order: PhantomData<O>,
}

impl<T: Ord, O: Order> SortedVec<T, O> {
    pub fn new() -> Self {
        todo!()
    }

    /// Inserts `x` after any items that compare equal to it.
    pub fn insert(&mut self, x: T) {
        todo!()
    }

    /// Whether some item compares equal to `x` under `O`.
    pub fn contains(&self, x: &T) -> bool {
        todo!()
    }

    pub fn as_slice(&self) -> &[T] {
        todo!()
    }

    /// The same items sorted by another order; equal items keep their relative order.
    pub fn reorder<O2: Order>(self) -> SortedVec<T, O2> {
        todo!()
    }
}

impl<T: Ord, O: Order> FromIterator<T> for SortedVec<T, O> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        todo!()
    }
}
"""

JOB = """
/// Ordered by priority only, so equal-priority jobs are distinguishable.
#[derive(Debug, PartialEq, Eq)]
struct Job(u8, &'static str);

impl PartialOrd for Job {
    fn partial_cmp(&self, o: &Job) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Job {
    fn cmp(&self, o: &Job) -> std::cmp::Ordering {
        self.0.cmp(&o.0)
    }
}

/// A test-defined strategy: everything is equal.
#[allow(dead_code)]
struct Unordered;

impl Order for Unordered {
    fn cmp<T: Ord>(_: &T, _: &T) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
}

fn names<O: Order>(v: &SortedVec<Job, O>) -> Vec<&'static str> {
    v.as_slice().iter().map(|j| j.1).collect()
}
"""

P.append(write(
    "zero-sized-strategies", "Zero-sized strategy types", "medium", "compile-time-rust", ["zero-sized types", "PhantomData", "default type parameters"],
    """
        `SortedVec<T, O>` keeps its items sorted by the strategy `O`, a zero-sized type: `Asc` (the default, so
        `SortedVec<T>` means ascending) or `Desc`, or one the caller defines. Because `O` is a type, a
        `SortedVec<i32, Desc>` is exactly as big as a `Vec<i32>`.

        - `insert` puts `x` **after** any items that compare equal to it, in O(log n) comparisons.
        - `contains` says whether some item compares equal to `x` under `O`, in O(log n).
        - `reorder::<O2>()` re-sorts into another strategy; equal items keep their relative order.
        - `collect()` builds one by inserting each item in turn.
    """,
    SORTED_STARTER,
    sorted_vec(),
    [JOB,
     T("default_is_ascending", "SortedVec<i32> from [3, 1, 2]", "v.as_slice()", "[1, 2, 3]", setup="let v: SortedVec<i32> = vec![3, 1, 2].into_iter().collect();"),
     T("descending", "SortedVec<i32, Desc> from [3, 1, 2]", "v.as_slice()", "[3, 2, 1]", setup="let v: SortedVec<i32, Desc> = vec![3, 1, 2].into_iter().collect();"),
     T("equal_items_keep_insertion_order", "jobs (1, a), (0, b), (1, c), (0, d)", "names(&v)", 'vec!["b", "d", "a", "c"]',
       setup='let v: SortedVec<Job> = vec![Job(1, "a"), Job(0, "b"), Job(1, "c"), Job(0, "d")].into_iter().collect();'),
     T("reorder_is_stable", "the same jobs, reordered to Desc", "names(&v.reorder::<Desc>())", 'vec!["a", "c", "b", "d"]',
       setup='let v: SortedVec<Job> = vec![Job(1, "a"), Job(0, "b"), Job(1, "c"), Job(0, "d")].into_iter().collect();'),
     T("zero_cost", "size_of SortedVec<u64, Desc> vs Vec<u64>", "(std::mem::size_of::<SortedVec<u64, Desc>>(), std::mem::size_of::<Asc>())", "(std::mem::size_of::<Vec<u64>>(), 0)")],
    [JOB,
     T("contains", "[1, 5, 9] contains 5 and 6", "(v.contains(&5), v.contains(&6))", "(true, false)", setup="let v: SortedVec<i32> = vec![9, 1, 5].into_iter().collect();"),
     T("contains_desc", "Desc [1, 5, 9] contains 1 and 0", "(v.contains(&1), v.contains(&0))", "(true, false)", setup="let v: SortedVec<i32, Desc> = vec![9, 1, 5].into_iter().collect();"),
     T("empty", "new SortedVec<String>", "(v.as_slice().len(), v.contains(&String::new()))", "(0, false)", setup="let v: SortedVec<String> = SortedVec::new();"),
     T("strings", 'insert "pear", "Apple", "fig"', "v.as_slice()", '["Apple", "fig", "pear"]',
       setup='let mut v = SortedVec::<&str>::new();\nfor w in ["pear", "Apple", "fig"] {\n    v.insert(w);\n}'),
     T("your_own_strategy", "Unordered keeps insertion order", "names(&v)", 'vec!["x", "y", "z"]',
       setup='let v: SortedVec<Job, Unordered> = vec![Job(2, "x"), Job(0, "y"), Job(1, "z")].into_iter().collect();'),
     T("reorder_back_and_forth", "[2, 1, 3] asc -> desc -> asc", "v.reorder::<Desc>().reorder::<Asc>().as_slice().to_vec()", "vec![1, 2, 3]",
       setup="let v: SortedVec<i32> = vec![2, 1, 3].into_iter().collect();"),
     T("duplicates_in_desc", "Desc jobs (0, a), (1, b), (0, c)", "names(&v)", 'vec!["b", "a", "c"]',
       setup='let v: SortedVec<Job, Desc> = vec![Job(0, "a"), Job(1, "b"), Job(0, "c")].into_iter().collect();'),
     T("zero_sized_strategies", "size_of Asc, Desc, Unordered", "(std::mem::size_of::<Asc>(), std::mem::size_of::<Desc>(), std::mem::size_of::<Unordered>())", "(0, 0, 0)"),
     T("extremes", "i64::MIN, i64::MAX, 0", "v.as_slice()", "[i64::MIN, 0, i64::MAX]", setup="let v: SortedVec<i64> = vec![i64::MAX, i64::MIN, 0].into_iter().collect();"),
     """
     #[test]
     fn random_vs_stable_sort() {
         let mut rng = anneal_prelude::Rng::new(4513);
         let tags = ["a", "b", "c", "d", "e", "f", "g", "h"];
         for _ in 0..300 {
             let n = rng.below(8);
             let pris: Vec<u8> = rng.vec(n, 0, 3);
             let jobs = || pris.iter().zip(tags).map(|(&p, t)| Job(p, t));
             let asc: SortedVec<Job> = jobs().collect();
             let desc: SortedVec<Job, Desc> = jobs().collect();
             let mut want_asc: Vec<Job> = jobs().collect();
             want_asc.sort_by(|a, b| a.0.cmp(&b.0));
             let mut want_desc: Vec<Job> = jobs().collect();
             want_desc.sort_by(|a, b| b.0.cmp(&a.0));
             let probe = Job(rng.int(0, 4) as u8, "?");
             let has = pris.contains(&probe.0);
             check!(format!("priorities = {pris:?}, probe = {}", probe.0), (names(&asc), names(&desc), asc.contains(&probe), desc.contains(&probe)),
                    (want_asc.iter().map(|j| j.1).collect::<Vec<_>>(), want_desc.iter().map(|j| j.1).collect::<Vec<_>>(), has, has));
         }
     }

     #[test]
     fn scale_insert_and_lookup() {
         let n = 200_000u64;
         let v: SortedVec<u64> = (0..n).map(|x| x * 2).collect();
         let found = (0..2 * n).filter(|x| v.contains(x)).count();
         check!("200000 sorted inserts, 400000 lookups", (v.as_slice().len(), found), (200_000, 200_000));
     }
     """],
    [("rust", "`PhantomData<O>` puts `O` in the type at zero size; call the strategy as `O::cmp(a, b)`. `SortedVec<T, O = Asc>` is a default type parameter, like `HashMap<K, V, S = RandomState>`."),
     ("approach", "Insert at `partition_point(|y| O::cmp(y, &x) != Ordering::Greater)`: the first position after every item that's not greater. `binary_search_by` returns *some* equal position, which breaks the order of equal items."),
     ("rust", "`reorder` moves the items into a `SortedVec<T, O2>`, a new type; `sort_by` is stable, so equal items keep their order.")],
    ("""Zero-sized strategy types are how std parameterises behaviour without a runtime cost: `HashMap<K, V, S = RandomState>`, `Vec<T, A = Global>`. The strategy is picked at compile time and monomorphized into the code, so there's no function pointer and no field. Syntax to remember: `pub struct SortedVec<T, O = Asc> { items: Vec<T>, order: PhantomData<O> }` and `v.reorder::<Desc>()`.""", "O(log n) comparisons per insert plus the O(n) shift; O(log n) contains", "O(n)"),
    "`HashMap::new()` only exists for `S = RandomState`, while `with_hasher` works for any `S`. Why can't `new` be generic over `S`?",
    ["Zero-sized strategy types in `PhantomData`.", "Default type parameters on a struct: `SortedVec<T, O = Asc>`.", "Stable insertion with `partition_point`."],
    related=("L5", "S4", "D4"),
    wrong=dict(
        unstable_insert=sorted_vec(pos="match self.items.binary_search_by(|y| O::cmp(y, &x)) { Ok(i) | Err(i) => i }"),
        linear_contains=sorted_vec(contains="self.items.iter().any(|y| O::cmp(y, x) == Ordering::Equal)"),
        insert_before_equals=sorted_vec(pos="self.items.partition_point(|y| O::cmp(y, &x) == Ordering::Less)"),
    ),
))


def matrix(transpose=None, mul_index="self.rows[i][k].clone() * rhs.rows[k][j].clone()", identity_cell="if r == c { one.clone() } else { T::default() }"):
    transpose = transpose or """        let mut rows = self.rows.map(|row| row.into_iter());
        Matrix { rows: std::array::from_fn(|_| std::array::from_fn(|r| rows[r].next().unwrap())) }"""
    return f"""
use std::ops::{{Add, Mul}};

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T, const R: usize, const C: usize> {{
    rows: [[T; C]; R],
}}

impl<T, const R: usize, const C: usize> Matrix<T, R, C> {{
    pub const ROWS: usize = R;
    pub const COLS: usize = C;

    pub fn from_rows(rows: [[T; C]; R]) -> Self {{
        Matrix {{ rows }}
    }}

    pub fn into_rows(self) -> [[T; C]; R] {{
        self.rows
    }}

    pub fn get(&self, r: usize, c: usize) -> Option<&T> {{
        self.rows.get(r)?.get(c)
    }}

    /// Moves every element; needs nothing from T.
    pub fn transpose(self) -> Matrix<T, C, R> {{
{transpose}
    }}
}}

/// Only square matrices have an identity.
impl<T: Clone + Default, const N: usize> Matrix<T, N, N> {{
    pub fn identity(one: T) -> Self {{
        Matrix {{ rows: std::array::from_fn(|r| std::array::from_fn(|c| {identity_cell})) }}
    }}
}}

impl<T: Add<Output = T>, const R: usize, const C: usize> Add for Matrix<T, R, C> {{
    type Output = Matrix<T, R, C>;

    fn add(self, rhs: Self) -> Self::Output {{
        let mut right = rhs.rows.map(|row| row.into_iter());
        let mut r = 0;
        Matrix {{
            rows: self.rows.map(|row| {{
                let out = row.map(|x| x + right[r].next().unwrap());
                r += 1;
                out
            }}),
        }}
    }}
}}

/// (R x K) * (K x C) = (R x C). A shape mismatch is a type error.
impl<T, const R: usize, const K: usize, const C: usize> Mul<Matrix<T, K, C>> for Matrix<T, R, K>
where
    T: Clone + Default + Add<Output = T> + Mul<Output = T>,
{{
    type Output = Matrix<T, R, C>;

    fn mul(self, rhs: Matrix<T, K, C>) -> Self::Output {{
        Matrix {{
            rows: std::array::from_fn(|i| {{
                std::array::from_fn(|j| {{
                    let mut sum = T::default();
                    for k in 0..K {{
                        sum = sum + {mul_index};
                    }}
                    sum
                }})
            }}),
        }}
    }}
}}
"""


MATRIX_STARTER = """
use std::ops::{Add, Mul};

#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T, const R: usize, const C: usize> {
    rows: [[T; C]; R],
}

impl<T, const R: usize, const C: usize> Matrix<T, R, C> {
    pub const ROWS: usize = R;
    pub const COLS: usize = C;

    pub fn from_rows(rows: [[T; C]; R]) -> Self {
        todo!()
    }

    pub fn into_rows(self) -> [[T; C]; R] {
        todo!()
    }

    pub fn get(&self, r: usize, c: usize) -> Option<&T> {
        todo!()
    }

    /// Moves every element; needs nothing from T.
    pub fn transpose(self) -> Matrix<T, C, R> {
        todo!()
    }
}

/// Only square matrices have an identity.
impl<T: Clone + Default, const N: usize> Matrix<T, N, N> {
    pub fn identity(one: T) -> Self {
        todo!()
    }
}

impl<T: Add<Output = T>, const R: usize, const C: usize> Add for Matrix<T, R, C> {
    type Output = Matrix<T, R, C>;

    fn add(self, rhs: Self) -> Self::Output {
        todo!()
    }
}

/// (R x K) * (K x C) = (R x C). A shape mismatch is a type error.
impl<T, const R: usize, const K: usize, const C: usize> Mul<Matrix<T, K, C>> for Matrix<T, R, K>
where
    T: Clone + Default + Add<Output = T> + Mul<Output = T>,
{
    type Output = Matrix<T, R, C>;

    fn mul(self, rhs: Matrix<T, K, C>) -> Self::Output {
        todo!()
    }
}
"""

MOVE_ONLY = """
/// Neither Clone nor Copy nor Default.
#[allow(dead_code)]
#[derive(Debug, PartialEq)]
struct Cell(u32);

/// A number that is Clone but not Copy.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Default)]
struct Big(i128);

impl std::ops::Add for Big {
    type Output = Big;
    fn add(self, o: Big) -> Big {
        Big(self.0 + o.0)
    }
}

impl std::ops::Mul for Big {
    type Output = Big;
    fn mul(self, o: Big) -> Big {
        Big(self.0 * o.0)
    }
}
"""

P.append(write(
    "shape-typed-matrix", "A matrix whose shape is its type", "medium", "compile-time-rust", ["const generics", "associated consts", "array::from_fn", "operator traits"],
    """
        `Matrix<T, R, C>` stores `[[T; C]; R]`, so multiplying a 2×3 by a 2×3 is a type error, not a panic. Fill in
        the methods and operators; the signatures and bounds are given.

        - `get(r, c)` is `None` out of range.
        - `transpose` must **move** the elements: it needs nothing from `T`, and the tests transpose a type that
          can't be cloned.
        - `identity(one)` exists only for square matrices; off-diagonal cells are `T::default()`.
        - `+` is element-wise; `*` is the matrix product.
    """,
    MATRIX_STARTER,
    matrix(),
    [MOVE_ONLY,
     T("multiply_shapes", "(2x3) * (3x2)", "(a * b).into_rows()", "[[58, 64], [139, 154]]",
       setup="let a = Matrix::from_rows([[1, 2, 3], [4, 5, 6]]);\nlet b = Matrix::from_rows([[7, 8], [9, 10], [11, 12]]);"),
     T("result_type_is_2_by_4", "(2x3) * (3x4) has type Matrix<i32, 2, 4>", "(Matrix::<i32, 2, 4>::ROWS, Matrix::<i32, 2, 4>::COLS, p.get(1, 3).copied())", "(2, 4, Some(0))",
       setup="let a = Matrix::from_rows([[1, 0, 0], [0, 1, 0]]);\nlet b: Matrix<i32, 3, 4> = Matrix::from_rows([[0; 4]; 3]);\nlet p: Matrix<i32, 2, 4> = a * b;"),
     T("transpose_moves", "transpose a 2x3 of non-Clone Cells", "m.transpose().into_rows()", "[[Cell(1), Cell(4)], [Cell(2), Cell(5)], [Cell(3), Cell(6)]]",
       setup="let m = Matrix::from_rows([[Cell(1), Cell(2), Cell(3)], [Cell(4), Cell(5), Cell(6)]]);"),
     T("identity", "Matrix::<i32, 3, 3>::identity(1)", "Matrix::<i32, 3, 3>::identity(1).into_rows()", "[[1, 0, 0], [0, 1, 0], [0, 0, 1]]"),
     T("get_out_of_range", "2x2: get(0, 1), get(2, 0), get(0, 2)", "(m.get(0, 1).copied(), m.get(2, 0), m.get(0, 2))", "(Some(2), None, None)",
       setup="let m = Matrix::from_rows([[1, 2], [3, 4]]);")],
    [MOVE_ONLY,
     T("add", "[[1, 2]] + [[10, 20]]", "(Matrix::from_rows([[1, 2]]) + Matrix::from_rows([[10, 20]])).into_rows()", "[[11, 22]]"),
     T("identity_is_neutral", "I * A == A for a 2x2 of f64", "(Matrix::<f64, 2, 2>::identity(1.0) * a.clone()) == a", "true",
       setup="let a = Matrix::from_rows([[1.5, -2.0], [0.25, 4.0]]);"),
     T("big_numbers", "Big (non-Copy) 2x2 product", "(a * b).into_rows()", "[[Big(19), Big(22)], [Big(43), Big(50)]]",
       setup="let a = Matrix::from_rows([[Big(1), Big(2)], [Big(3), Big(4)]]);\nlet b = Matrix::from_rows([[Big(5), Big(6)], [Big(7), Big(8)]]);"),
     T("row_times_column", "(1x3) * (3x1)", "(Matrix::from_rows([[1, 2, 3]]) * Matrix::from_rows([[4], [5], [6]])).into_rows()", "[[32]]"),
     T("column_times_row", "(2x1) * (1x2)", "(Matrix::from_rows([[1], [2]]) * Matrix::from_rows([[3, 4]])).into_rows()", "[[3, 4], [6, 8]]"),
     T("transpose_twice", "transpose twice gives the original", "m.clone().transpose().transpose() == m", "true",
       setup='let m = Matrix::from_rows([["a", "b", "c"], ["d", "e", "f"]]);'),
     T("empty_shapes", "a 0x3 matrix transposed is 3x0", "(Matrix::<u8, 3, 0>::ROWS, m.transpose().into_rows().len())", "(3, 3)",
       setup="let m: Matrix<u8, 0, 3> = Matrix::from_rows([]);"),
     T("zero_inner_dimension", "(2x0) * (0x2) is all zeros", "(a * b).into_rows()", "[[0, 0], [0, 0]]",
       setup="let a: Matrix<i32, 2, 0> = Matrix::from_rows([[], []]);\nlet b: Matrix<i32, 0, 2> = Matrix::from_rows([]);"),
     T("identity_of_big", "Matrix::<Big, 2, 2>::identity(Big(1))", "Matrix::<Big, 2, 2>::identity(Big(1)).into_rows()", "[[Big(1), Big(0)], [Big(0), Big(1)]]"),
     T("wrapping", "Wrapping<u8> product overflows per cell", "(a * b).into_rows()", "[[Wrapping(0u8)]]",
       setup="use std::num::Wrapping;\nlet a = Matrix::from_rows([[Wrapping(16u8), Wrapping(16u8)]]);\nlet b = Matrix::from_rows([[Wrapping(8u8)], [Wrapping(8u8)]]);"),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4514);
         for _ in 0..300 {
             let mut a = [[0i64; 3]; 2];
             let mut b = [[0i64; 4]; 3];
             for row in a.iter_mut() {
                 for x in row.iter_mut() {
                     *x = rng.int(-9, 9);
                 }
             }
             for row in b.iter_mut() {
                 for x in row.iter_mut() {
                     *x = rng.int(-9, 9);
                 }
             }
             let mut want = [[0i64; 4]; 2];
             for i in 0..2 {
                 for j in 0..4 {
                     for k in 0..3 {
                         want[i][j] += a[i][k] * b[k][j];
                     }
                 }
             }
             let mut want_t = [[0i64; 2]; 3];
             for i in 0..2 {
                 for j in 0..3 {
                     want_t[j][i] = a[i][j];
                 }
             }
             let ma = Matrix::from_rows(a);
             check!(format!("a = {a:?}, b = {b:?}"), ((ma.clone() * Matrix::from_rows(b)).into_rows(), ma.clone().transpose().into_rows(), (ma.clone() + ma).into_rows()),
                    (want, want_t, a.map(|row| row.map(|x| 2 * x))));
         }
     }
     """],
    [("rust", "Build arrays with `std::array::from_fn(|i| ...)`: it calls the closure once per index, in order, so it works for any `T`."),
     ("rust", "To move elements out of `[[T; C]; R]`, turn each row into an iterator (`rows.map(|row| row.into_iter())`) and pull `next()` from row `r` while building column-major output."),
     ("edge", "Zero-sized dimensions are legal: a `2×0` times a `0×2` is a `2×2` of `T::default()`.")],
    ("""With the shape in the type, the compiler checks dimensions once, when the code is written, and every size is known at compile time, so the arrays live inline without a heap allocation. Syntax to remember: `impl<T, const R: usize, const K: usize, const C: usize> Mul<Matrix<T, K, C>> for Matrix<T, R, K>`; a square-only impl is `impl<T, const N: usize> Matrix<T, N, N>`; associated consts `Matrix::<i32, 2, 4>::ROWS`.""", "O(R·K·C) multiply; O(R·C) transpose and add", "O(R·C), inline"),
    "When would you pick `Vec`-backed matrices with runtime shapes instead? What does each `Matrix<T, R, C>` instantiation cost in binary size?",
    ["Const generic shapes turn dimension bugs into type errors.", "`array::from_fn` and moving out of arrays with `into_iter`.", "Impls for a subset of shapes (`Matrix<T, N, N>`)."],
    related=("L5", "L4"),
    wrong=dict(
        product_indexes_wrong=matrix(mul_index="self.rows[i][k].clone() * rhs.rows[k][k % C].clone()"),
        identity_everywhere=matrix(identity_cell="one.clone()"),
    ),
))


# ---------------------------------------------------------------- type-level design (hard)

USER_HEAD = """
use std::marker::PhantomData;

/// 3 to 16 characters: a lowercase ASCII letter, then lowercase letters, digits or `_`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

pub struct LoggedOut;
pub struct LoggedIn;
pub struct Locked;

pub struct User<S> {
    name: Username,
    password: String,
    failures: u32,
    state: PhantomData<S>,
}

/// A failed login hands the user back, in whichever state it ended up.
pub enum LoginError {
    WrongPassword(User<LoggedOut>),
    Locked(User<Locked>),
}
"""


def user_solution(valid_tail="c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'", lock_at="3", reset_on_success="0", extra=""):
    return USER_HEAD + f"""
impl Username {{
    pub fn parse(s: &str) -> Option<Username> {{
        let mut chars = s.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        let ok = first_ok && (3..=16).contains(&s.len()) && chars.all(|c| {valid_tail});
        ok.then(|| Username(s.to_string()))
    }}

    pub fn as_str(&self) -> &str {{
        &self.0
    }}
}}
{extra}
impl<S> User<S> {{
    pub fn name(&self) -> &Username {{
        &self.name
    }}

    pub fn failures(&self) -> u32 {{
        self.failures
    }}

    // Changing the state parameter builds a new value; moving the fields keeps it free.
    fn into_state<T>(self) -> User<T> {{
        User {{ name: self.name, password: self.password, failures: self.failures, state: PhantomData }}
    }}
}}

impl User<LoggedOut> {{
    pub fn new(name: Username, password: &str) -> Self {{
        User {{ name, password: password.to_string(), failures: 0, state: PhantomData }}
    }}

    /// Right password: logged in, failures reset. Wrong: one more failure; the third in a row locks the account.
    pub fn login(mut self, password: &str) -> Result<User<LoggedIn>, LoginError> {{
        if password == self.password {{
            self.failures = {reset_on_success};
            return Ok(self.into_state());
        }}
        self.failures += 1;
        if self.failures >= {lock_at} {{
            Err(LoginError::Locked(self.into_state()))
        }} else {{
            Err(LoginError::WrongPassword(self))
        }}
    }}
}}

impl User<LoggedIn> {{
    pub fn dashboard(&self) -> String {{
        format!("dashboard:{{}}", self.name.as_str())
    }}

    pub fn change_password(&mut self, new: &str) {{
        self.password = new.to_string();
    }}

    pub fn logout(self) -> User<LoggedOut> {{
        self.into_state()
    }}
}}

impl User<Locked> {{
    /// An admin unlock: back to logged out with no failures.
    pub fn unlock(mut self) -> User<LoggedOut> {{
        self.failures = 0;
        self.into_state()
    }}
}}
"""


USER_STARTER = USER_HEAD + """
impl Username {
    pub fn parse(s: &str) -> Option<Username> {
        todo!()
    }

    pub fn as_str(&self) -> &str {
        todo!()
    }
}

impl<S> User<S> {
    pub fn name(&self) -> &Username {
        todo!()
    }

    pub fn failures(&self) -> u32 {
        todo!()
    }
}

impl User<LoggedOut> {
    pub fn new(name: Username, password: &str) -> Self {
        todo!()
    }

    /// Right password: logged in, failures reset. Wrong: one more failure; the third in a row locks the account.
    pub fn login(self, password: &str) -> Result<User<LoggedIn>, LoginError> {
        todo!()
    }
}

impl User<LoggedIn> {
    pub fn dashboard(&self) -> String {
        todo!()
    }

    pub fn change_password(&mut self, new: &str) {
        todo!()
    }

    pub fn logout(self) -> User<LoggedOut> {
        todo!()
    }
}

impl User<Locked> {
    /// An admin unlock: back to logged out with no failures.
    pub fn unlock(self) -> User<LoggedOut> {
        todo!()
    }
}
"""

PROBES = """
/// `Probe::<T>::DEFAULT` is true only if T: Default (inherent consts win over trait consts when their bounds hold).
#[allow(dead_code)]
struct Probe<T: ?Sized>(std::marker::PhantomData<T>);

#[allow(dead_code)]
trait Fallback {
    const DEFAULT: bool = false;
    const FROM_STR_REF: bool = false;
}

#[allow(dead_code)]
impl<T: ?Sized> Fallback for Probe<T> {}

#[allow(dead_code)]
impl<T: Default> Probe<T> {
    const DEFAULT: bool = true;
}

#[allow(dead_code)]
impl<T: for<'a> From<&'a str>> Probe<T> {
    const FROM_STR_REF: bool = true;
}
"""

USER_HELPERS = PROBES + """
fn user(name: &str, pw: &str) -> User<LoggedOut> {
    User::new(Username::parse(name).unwrap(), pw)
}

/// Which variant, and the failure count it carries.
fn outcome(r: Result<User<LoggedIn>, LoginError>) -> (&'static str, u32) {
    match r {
        Ok(u) => ("in", u.failures()),
        Err(LoginError::WrongPassword(u)) => ("wrong", u.failures()),
        Err(LoginError::Locked(u)) => ("locked", u.failures()),
    }
}

fn wrong(r: Result<User<LoggedIn>, LoginError>) -> User<LoggedOut> {
    match r {
        Err(LoginError::WrongPassword(u)) => u,
        _ => panic!("expected WrongPassword"),
    }
}
"""

P.append(write(
    "newtype-typestate", "Newtype + typestate login", "medium", "type-level-design", ["newtype", "typestate", "PhantomData"],
    """
        Model an account so misuse doesn't compile. A `Username` can only come from `Username::parse` (3 to 16
        characters: a lowercase ASCII letter, then lowercase letters, digits or `_`). A `User<S>` is in one of
        three states, and each state has only the methods that make sense in it:

        - `User<LoggedOut>`: `new`, and `login(pw)`. The right password returns a `User<LoggedIn>` and resets the
          failure count. A wrong one adds a failure and hands the user back in `LoginError::WrongPassword`; the
          **third failure in a row** hands it back as a `User<Locked>` in `LoginError::Locked`.
        - `User<LoggedIn>`: `dashboard()` (`"dashboard:<name>"`), `change_password`, `logout`.
        - `User<Locked>`: `unlock()`, back to logged out with no failures.
        - Every state: `name()` and `failures()`.

        There must be no other way to make a `Username`: no `Default`, no `From<&str>`.
    """,
    USER_STARTER,
    user_solution(),
    [USER_HELPERS,
     T("login_and_dashboard", 'user "ada" / "pw", login with "pw"', 'user("ada", "pw").login("pw").ok().map(|u| u.dashboard())', 'Some("dashboard:ada".to_string())'),
     T("third_failure_locks", "three wrong passwords", 'outcome(wrong(wrong(user("ada", "pw").login("x")).login("y")).login("z"))', '("locked", 3)'),
     T("success_resets_failures", "wrong, then right, then logout", 'wrong(user("ada", "pw").login("x")).login("pw").ok().map(|u| u.logout().failures())', "Some(0)"),
     T("username_rules", '"ada_1", "Ada", "adA", "ab", "1ab", "a-b"', '["ada_1", "Ada", "adA", "ab", "1ab", "a-b"].map(|s| Username::parse(s).is_some())', "[true, false, false, false, false, false]"),
     T("no_shortcuts_to_a_username", "is Username Default? From<&str>?", "(Probe::<Username>::DEFAULT, Probe::<Username>::FROM_STR_REF)", "(false, false)")],
    [USER_HELPERS,
     T("locked_then_unlocked", "three failures, then unlock", "(locked.failures(), locked.unlock().failures())", "(3, 0)",
       setup='let locked = match wrong(wrong(user("bob", "pw").login("1")).login("2")).login("3") {\n    Err(LoginError::Locked(u)) => u,\n    _ => panic!("expected Locked"),\n};'),
     T("unlocked_user_can_log_in", "lock, unlock, log in with the right password", "u.login(\"pw\").ok().map(|u| (u.dashboard(), u.failures()))", 'Some(("dashboard:bob".to_string(), 0))',
       setup='let locked = match wrong(wrong(user("bob", "pw").login("1")).login("2")).login("3") {\n    Err(LoginError::Locked(u)) => u,\n    _ => panic!("expected Locked"),\n};\nlet u = locked.unlock();'),
     T("change_password", "log in, change to \"new\", log out, log in with old then new", 'outcome(wrong(u.login("pw")).login("new"))', '("in", 0)',
       setup='let mut u = user("cyd", "pw").login("pw").ok().unwrap();\nu.change_password("new");\nlet u = u.logout();'),
     T("failures_count_up", "two wrong passwords", 'wrong(wrong(user("ada", "pw").login("a")).login("b")).failures()', "2"),
     T("password_is_case_sensitive", 'password "PW", login "pw"', 'outcome(user("ada", "PW").login("pw"))', '("wrong", 1)'),
     T("name_in_every_state", "name() while logged out and in", '(u.name().as_str().to_string(), u.login("p").ok().map(|u| u.name().clone()))', '("dora".to_string(), Username::parse("dora"))',
       setup='let u = user("dora", "p");'),
     T("length_limits", '3, 16 and 17 characters', '["abc", "a234567890123456", "a2345678901234567"].map(|s| Username::parse(s).is_some())', "[true, true, false]"),
     T("non_ascii_rejected", '"josé", "ünï", "abc "', '["josé", "ünï", "abc "].map(|s| Username::parse(s).is_some())', "[false, false, false]"),
     T("empty_and_underscore_start", '"", "_ab", "a__"', '["", "_ab", "a__"].map(|s| Username::parse(s).is_some())', "[false, false, true]"),
     T("zero_cost_states", "size_of User<LoggedOut>, User<LoggedIn>, User<Locked>", "(std::mem::size_of::<User<LoggedIn>>(), std::mem::size_of::<User<Locked>>(), std::mem::size_of::<LoggedIn>())",
       "(std::mem::size_of::<User<LoggedOut>>(), std::mem::size_of::<User<LoggedOut>>(), 0)"),
     """
     enum Any {
         Out(User<LoggedOut>),
         In(User<LoggedIn>),
         Lock(User<Locked>),
     }

     #[test]
     fn random_vs_state_model() {
         let mut rng = anneal_prelude::Rng::new(4515);
         for _ in 0..300 {
             let mut acct = Any::Out(user("eve", "a"));
             // model: 0 = out, 1 = in, 2 = locked
             let (mut state, mut failures, mut pw) = (0, 0u32, "a".to_string());
             let mut log = Vec::new();
             for _ in 0..rng.below(12) {
                 let guess = rng.string(1, "ab");
                 let fresh = rng.string(1, "ab");
                 acct = match acct {
                     Any::Out(u) => {
                         log.push(format!("login({guess})"));
                         if guess == pw {
                             state = 1;
                             failures = 0;
                         } else {
                             failures += 1;
                             state = if failures >= 3 { 2 } else { 0 };
                         }
                         match u.login(&guess) {
                             Ok(u) => Any::In(u),
                             Err(LoginError::WrongPassword(u)) => Any::Out(u),
                             Err(LoginError::Locked(u)) => Any::Lock(u),
                         }
                     }
                     Any::In(mut u) => {
                         if rng.bool() {
                             log.push(format!("change_password({fresh})"));
                             u.change_password(&fresh);
                             pw = fresh;
                             Any::In(u)
                         } else {
                             log.push("logout".to_string());
                             state = 0;
                             Any::Out(u.logout())
                         }
                     }
                     Any::Lock(u) => {
                         log.push("unlock".to_string());
                         state = 0;
                         failures = 0;
                         Any::Out(u.unlock())
                     }
                 };
                 let got = match &acct {
                     Any::Out(u) => (0, u.failures()),
                     Any::In(u) => (1, u.failures()),
                     Any::Lock(u) => (2, u.failures()),
                 };
                 check!(log.join(", "), got, (state, failures));
             }
         }
     }

     #[test]
     fn random_usernames_vs_rules() {
         let mut rng = anneal_prelude::Rng::new(4516);
         for _ in 0..400 {
             let n = rng.below(19);
             let s = rng.string(n, "az_09Aé-");
             let b = s.as_bytes();
             let want = (3..=16).contains(&b.len())
                 && b[0].is_ascii_lowercase()
                 && b.iter().all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_');
             check!(format!("{s:?}"), Username::parse(&s).map(|u| u.as_str().to_string()), want.then(|| s.clone()));
         }
     }
     """],
    [("rust", "Keep the `Username` field private so `parse` is the only constructor. Don't derive `Default` or add `From<&str>`."),
     ("rust", "A state change is a new value: `User<LoggedOut>` and `User<LoggedIn>` are different types. Write one private `fn into_state<T>(self) -> User<T>` on `impl<S> User<S>` that moves the fields across."),
     ("edge", "Count the failure *before* deciding: the third wrong password returns `Locked` with `failures() == 3`, and a success resets the count.")],
    ("""Two techniques that stack: a newtype with a private field makes 'validated' part of the type (parse, don't validate), and typestate makes 'which operations are legal now' part of the type. Because transitions take `self` by value, the old state can't be used after a transition, and a failed login still hands the user back instead of losing it. Syntax to remember: `impl<S> User<S> { fn into_state<T>(self) -> User<T> { User { name: self.name, ..., state: PhantomData } } }`; `..self` can't change the type parameter.""", "O(1) per transition; O(n) parse", "O(1) beyond the strings"),
    "When is typestate the wrong tool? Compare it with an enum state machine for an account whose state comes from a database row.",
    ["Newtypes with private fields for validated values.", "Typestate transitions that consume `self`, including fallible ones that hand the value back."],
    source="DP17", related=("L5", "L7", "M1"),
    wrong=dict(
        locks_on_second_failure=user_solution(lock_at="2"),
        keeps_failures_after_success=user_solution(reset_on_success="self.failures"),
        accepts_uppercase=user_solution(valid_tail="c.is_ascii_alphanumeric() || c == '_'"),
        default_username=user_solution(extra="\nimpl Default for Username {\n    fn default() -> Self {\n        Username(\"user\".to_string())\n    }\n}\n"),
    ),
))


def units(derives=False, marker="PhantomData<fn() -> U>", add_impl=None, to_body="Length::new(self.value * U::METERS / V::METERS)"):
    add_impl = add_impl or """/// Only lengths in the same unit add up; convert first with `to`.
impl<U> Add for Length<U> {
    type Output = Length<U>;
    fn add(self, rhs: Length<U>) -> Length<U> {
        Length { value: self.value + rhs.value, unit: PhantomData }
    }
}"""
    if derives:
        struct = f"""#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Length<U> {{
    value: f64,
    unit: {marker},
}}
"""
    else:
        struct = f"""// `U` is only a label. `PhantomData<fn() -> U>` records it without owning a `U`: `Length<U>` stays
// Send + Sync and covariant whatever `U` is. Derives would add `U: Clone`, `U: Debug`, ... bounds, so the
// traits are implemented by hand, with no bounds on `U`.
pub struct Length<U> {{
    value: f64,
    unit: {marker},
}}

impl<U> Clone for Length<U> {{
    fn clone(&self) -> Self {{
        *self
    }}
}}

impl<U> Copy for Length<U> {{}}

impl<U> PartialEq for Length<U> {{
    fn eq(&self, other: &Self) -> bool {{
        self.value == other.value
    }}
}}

impl<U> PartialOrd for Length<U> {{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {{
        self.value.partial_cmp(&other.value)
    }}
}}

impl<U: Unit> fmt::Debug for Length<U> {{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{
        write!(f, "{{}} {{}}", self.value, U::SYMBOL)
    }}
}}
"""
    return f"""
use std::fmt;
use std::marker::PhantomData;
use std::ops::{{Add, Div, Mul}};

pub trait Unit {{
    /// How many metres one of this unit is.
    const METERS: f64;
    const SYMBOL: &'static str;
}}

pub struct Meters;
pub struct Feet;
pub struct Kilometers;

impl Unit for Meters {{
    const METERS: f64 = 1.0;
    const SYMBOL: &'static str = "m";
}}

impl Unit for Feet {{
    const METERS: f64 = 0.3048;
    const SYMBOL: &'static str = "ft";
}}

impl Unit for Kilometers {{
    const METERS: f64 = 1000.0;
    const SYMBOL: &'static str = "km";
}}

{struct}
impl<U: Unit> Length<U> {{
    pub fn new(value: f64) -> Self {{
        Length {{ value, unit: PhantomData }}
    }}

    pub fn value(self) -> f64 {{
        self.value
    }}

    /// The same length in another unit.
    pub fn to<V: Unit>(self) -> Length<V> {{
        {to_body}
    }}
}}

{add_impl}

impl<U> Mul<f64> for Length<U> {{
    type Output = Length<U>;
    fn mul(self, k: f64) -> Length<U> {{
        Length {{ value: self.value * k, unit: PhantomData }}
    }}
}}

/// How many times `rhs` fits into `self`.
impl<U> Div for Length<U> {{
    type Output = f64;
    fn div(self, rhs: Length<U>) -> f64 {{
        self.value / rhs.value
    }}
}}

impl<U: Unit> fmt::Display for Length<U> {{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{
        write!(f, "{{}} {{}}", self.value, U::SYMBOL)
    }}
}}
"""


UNIT_PROBES = """
/// Test-defined units: `Inches` derives nothing; `Handle` holds a raw pointer, so it's neither Send nor Sync.
#[allow(dead_code)]
struct Inches;

#[allow(dead_code)]
impl Unit for Inches {
    const METERS: f64 = 0.0254;
    const SYMBOL: &'static str = "in";
}

#[allow(dead_code)]
struct Handle(*const u8);

#[allow(dead_code)]
impl Unit for Handle {
    const METERS: f64 = 2.0;
    const SYMBOL: &'static str = "h";
}

/// Compile-time facts as runtime booleans: an inherent const wins over the trait's when its bounds hold.
#[allow(dead_code)]
struct Probe<A, B = ()>(std::marker::PhantomData<(A, B)>);

#[allow(dead_code)]
trait Fallback {
    const ADDS: bool = false;
    const SEND_SYNC: bool = false;
}

#[allow(dead_code)]
impl<A, B> Fallback for Probe<A, B> {}

#[allow(dead_code)]
impl<A: std::ops::Add<B>, B> Probe<A, B> {
    const ADDS: bool = true;
}

#[allow(dead_code)]
impl<A: Send + Sync> Probe<A> {
    const SEND_SYNC: bool = true;
}

#[allow(dead_code)]
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9 * b.abs().max(1.0)
}
"""

P.append(fix(
    "phantom-units-of-measure", "PhantomData units of measure", "hard", "type-level-design", ["PhantomData", "variance", "derive bounds", "associated consts"],
    """
        `Length<U>` is an `f64` tagged with a unit. The unit exists only in the type: `Length<Meters>` and
        `Length<Feet>` are different types, and `to::<V>()` converts. It has three problems:

        - Adding a `Length<Feet>` to a `Length<Meters>` compiles and adds the raw numbers. Make it a type error.
        - It doesn't compile for the tests' own units. `Length<U>` must be `Copy`, `Clone`, `PartialEq`,
          `PartialOrd` and `Debug` (printed like `Display`: `"2.5 in"`) for **every** `U: Unit`, including units
          that derive nothing.
        - `Length<Handle>` isn't `Send` or `Sync`, because `Handle` holds a raw pointer. A length never contains a
          unit value, so it must be `Send + Sync` whatever `U` is.
    """,
    units(derives=True, marker="PhantomData<U>", add_impl="""/// Adds two lengths.
impl<U, V> Add<Length<V>> for Length<U> {
    type Output = Length<U>;
    fn add(self, rhs: Length<V>) -> Length<U> {
        Length { value: self.value + rhs.value, unit: PhantomData }
    }
}"""),
    units(),
    [UNIT_PROBES,
     T("same_unit_adds", "2.5 m + 1 m", "(Length::<Meters>::new(2.5) + Length::<Meters>::new(1.0)).value()", "3.5"),
     T("mixed_units_dont_add", "does Length<Meters> + Length<Feet> compile? and Meters + Meters?", "(Probe::<Length<Meters>, Length<Feet>>::ADDS, Probe::<Length<Meters>, Length<Meters>>::ADDS)", "(false, true)"),
     T("convert_then_add", "1 km + 500 m, in metres", "close((Length::<Kilometers>::new(1.0).to::<Meters>() + Length::<Meters>::new(500.0)).value(), 1500.0)", "true"),
     T("copy_for_any_unit", "Inches (no derives): a + a, compare, Debug", "(a + a == Length::new(5.0), a < a * 2.0, format!(\"{a:?}\"))", '(true, true, "2.5 in".to_string())',
       setup="let a = Length::<Inches>::new(2.5);"),
     T("send_sync_for_any_unit", "is Length<Handle> Send + Sync?", "(Probe::<Length<Handle>>::SEND_SYNC, Probe::<Length<Meters>>::SEND_SYNC)", "(true, true)")],
    [UNIT_PROBES,
     T("feet_to_meters", "10 ft in m", "close(Length::<Feet>::new(10.0).to::<Meters>().value(), 3.048)", "true"),
     T("inches_to_feet", "12 in in ft", "close(Length::<Inches>::new(12.0).to::<Feet>().value(), 1.0)", "true"),
     T("round_trip", "3.7 km -> ft -> km", "close(Length::<Kilometers>::new(3.7).to::<Feet>().to::<Kilometers>().value(), 3.7)", "true"),
     T("display", "Display of 2 ft and 0.5 km", '(Length::<Feet>::new(2.0).to_string(), Length::<Kilometers>::new(0.5).to_string())', '("2 ft".to_string(), "0.5 km".to_string())'),
     T("ratio", "3 m / 1.5 m", "Length::<Meters>::new(3.0) / Length::<Meters>::new(1.5)", "2.0"),
     T("feet_and_km_dont_add", "Length<Feet> + Length<Kilometers>? Length<Inches> + Length<Inches>?", "(Probe::<Length<Feet>, Length<Kilometers>>::ADDS, Probe::<Length<Inches>, Length<Inches>>::ADDS)", "(false, true)"),
     T("handle_lengths_work", "Length<Handle>: 1 h to m, and copy", "(close(h.to::<Meters>().value(), 2.0), (h + h).value())", "(true, 2.0)", setup="let h = Length::<Handle>::new(1.0);"),
     T("debug_of_feet", "{:?} of -1.5 ft", 'format!("{:?}", Length::<Feet>::new(-1.5))', '"-1.5 ft"'),
     T("zero_sized_label", "size_of Length<Handle> and Length<Inches>", "(std::mem::size_of::<Length<Handle>>(), std::mem::size_of::<Length<Inches>>())", "(8, 8)"),
     T("scale_and_compare", "Inches: 3 * 2 vs 5", "(Length::<Inches>::new(3.0) * 2.0 > Length::new(5.0), Length::<Inches>::new(3.0) == Length::new(3.0))", "(true, true)"),
     """
     #[test]
     fn random_conversions() {
         let mut rng = anneal_prelude::Rng::new(4517);
         for _ in 0..300 {
             let x = rng.int(-100_000, 100_000) as f64 / 100.0;
             let y = rng.int(-100_000, 100_000) as f64 / 100.0;
             let f = Length::<Feet>::new(x);
             let sum = (f.to::<Meters>() + Length::<Meters>::new(y)).value();
             check!(format!("{x} ft + {y} m"), (close(sum, x * 0.3048 + y), close(f.to::<Inches>().value(), x * 12.0), close(f.to::<Kilometers>().to::<Feet>().value(), x)), (true, true, true));
         }
     }
     """],
    [("rust", "Mixed units: replace `impl<U, V> Add<Length<V>> for Length<U>` with `impl<U> Add for Length<U>` (`Rhs` defaults to `Self`)."),
     ("rust", "`#[derive(Clone)]` on `Length<U>` generates `impl<U: Clone> Clone for Length<U>`, even though no `U` is stored. Write `Clone`, `Copy`, `PartialEq`, `PartialOrd` and `Debug` by hand with no bound on `U` (`Debug` can use `U: Unit`)."),
     ("rust", "`PhantomData<U>` acts as if a `U` were stored: it inherits `U`'s `Send`/`Sync` and drop-check behaviour. `PhantomData<fn() -> U>` says 'produces a U', which is always `Send + Sync` and still covariant.")],
    ("""`PhantomData<X>` tells the compiler to treat the struct as if it contained an `X`, for auto traits (`Send`, `Sync`), variance and drop check. Pick `X` to say what you mean: `PhantomData<U>` owns a `U`; `PhantomData<fn() -> U>` produces one (covariant, always Send + Sync, no drop-check obligation); `PhantomData<fn(U)>` consumes one (contravariant); `PhantomData<*const U>` is `!Send`. Derives bound every type parameter, so marker-typed structs usually implement their traits by hand. Syntax to remember: `impl<U> Add for Length<U>`, `pub trait Unit { const METERS: f64; }`, `U::METERS`, `x.to::<Feet>()`.""", "O(1) per operation", "one f64"),
    "What is the variance of `Length<U>` in `U` with `PhantomData<U>`, `PhantomData<fn() -> U>` and `PhantomData<fn(U)>`? When does it matter?",
    ["`PhantomData` choices: ownership, auto traits and variance.", "Derives add bounds on phantom parameters; hand-written impls don't.", "Associated consts as per-type data."],
    rules=dict(unsafe=True),
    related=("L5", "S8", "C1"),
    wrong=dict(
        converting_add=units(add_impl="""/// Adds two lengths, converting the right-hand side.
impl<U: Unit, V: Unit> Add<Length<V>> for Length<U> {
    type Output = Length<U>;
    fn add(self, rhs: Length<V>) -> Length<U> {
        Length { value: self.value + rhs.to::<U>().value, unit: PhantomData }
    }
}"""),
        owns_a_u=units(marker="PhantomData<U>"),
        converts_backwards=units(to_body="Length::new(self.value * V::METERS / U::METERS)"),
    ),
))


def counts(read_body=None, order="b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))", split="text.split_whitespace()"):
    read_body = read_body or """    // read_to_end retries on ErrorKind::Interrupted and joins every chunk before decoding,
    // so a character split across two reads is decoded whole.
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let text = String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;"""
    return f"""
use std::collections::HashMap;
use std::io::{{self, Read}};

/// Word counts, most frequent first; ties in byte order of the word.
/// Generic, so it's compiled once per reader type: keep it to one line.
pub fn word_counts<R: Read>(mut reader: R) -> io::Result<Vec<(String, usize)>> {{
    word_counts_dyn(&mut reader)
}}

/// The worker. It isn't generic, so it's compiled once, whatever readers callers use.
pub fn word_counts_dyn(reader: &mut dyn Read) -> io::Result<Vec<(String, usize)>> {{
{read_body}
    let mut counts: HashMap<String, usize> = HashMap::new();
    for w in {split} {{
        *counts.entry(w.to_string()).or_insert(0) += 1;
    }}
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| {order});
    Ok(out)
}}
"""


COUNTS_STARTER = """
use std::collections::HashMap;
use std::io::{self, Read};

/// Word counts, most frequent first; ties in byte order of the word.
pub fn word_counts<R: Read>(mut reader: R) -> io::Result<Vec<(String, usize)>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut buf = [0u8; 4096];
    let mut partial = String::new();
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let chunk = std::str::from_utf8(&buf[..n]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        partial.push_str(chunk);
        // Count every complete word; keep a word that may continue in the next chunk.
        let keep_from = partial.rfind(char::is_whitespace).map_or(0, |i| i + partial[i..].chars().next().unwrap().len_utf8());
        for w in partial[..keep_from].split_whitespace() {
            *counts.entry(w.to_string()).or_insert(0) += 1;
        }
        partial = partial[keep_from..].to_string();
    }
    for w in partial.split_whitespace() {
        *counts.entry(w.to_string()).or_insert(0) += 1;
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by_key(|(w, n)| (std::cmp::Reverse(*n), w.clone()));
    Ok(out)
}
"""

READERS = """
use std::io::{self, Read};

/// Hands out at most `step` bytes per read.
#[allow(dead_code)]
struct Trickle<'a> {
    data: &'a [u8],
    step: usize,
}

#[allow(dead_code)]
impl Read for Trickle<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.step.min(buf.len()).min(self.data.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

/// Fails every other read with ErrorKind::Interrupted, which callers are expected to retry.
#[allow(dead_code)]
struct Flaky<'a> {
    data: &'a [u8],
    fail_next: bool,
}

#[allow(dead_code)]
impl Read for Flaky<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.fail_next = !self.fail_next;
        if !self.fail_next {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "try again"));
        }
        let n = 3.min(buf.len()).min(self.data.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

/// Gives `data`, then fails for real.
#[allow(dead_code)]
struct Broken<'a> {
    data: &'a [u8],
}

#[allow(dead_code)]
impl Read for Broken<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::ConnectionReset, "gone"));
        }
        let n = buf.len().min(self.data.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

#[allow(dead_code)]
fn pairs(v: &[(&str, usize)]) -> Vec<(String, usize)> {
    v.iter().map(|&(w, n)| (w.to_string(), n)).collect()
}
"""

P.append(fix(
    "inner-fn-monomorphization", "Monomorphization cost and the inner-fn trick", "hard", "type-level-design", ["monomorphization", "dyn vs generics", "io::Read"],
    """
        `word_counts` counts words (split on Unicode whitespace, case-sensitive) and returns them most frequent
        first, ties in byte order. It's generic over `R: Read`, so its whole body is compiled again for every reader
        type a program uses.

        - Move the work into a **non-generic** `pub fn word_counts_dyn(reader: &mut dyn Read)` with the same
          result type, and make `word_counts` a one-line forwarder. The tests use `word_counts_dyn` as a plain `fn`
          pointer.
        - It also breaks on readers that hand out a few bytes at a time or return `ErrorKind::Interrupted` (which
          means "retry"). Fix those. Input that isn't UTF-8 fails with `ErrorKind::InvalidData`; other read errors
          are passed on.
    """,
    COUNTS_STARTER,
    counts(),
    [READERS,
     T("counts_sorted", '"b a b c a b"', 'word_counts("b a b c a b".as_bytes()).unwrap()', 'pairs(&[("b", 3), ("a", 2), ("c", 1)])'),
     T("non_generic_worker", "word_counts_dyn as a fn pointer, on a Cursor", 'f(&mut io::Cursor::new("x y x")).unwrap()', 'pairs(&[("x", 2), ("y", 1)])',
       setup="let f: fn(&mut dyn Read) -> io::Result<Vec<(String, usize)>> = word_counts_dyn;"),
     T("one_byte_at_a_time", '"héllo wörld héllo", one byte per read', 'word_counts(Trickle { data: "héllo wörld héllo".as_bytes(), step: 1 }).unwrap()', 'pairs(&[("héllo", 2), ("wörld", 1)])'),
     T("interrupted_is_retried", '"to be or not to be", Interrupted every other read', 'word_counts(Flaky { data: b"to be or not to be", fail_next: false }).unwrap()',
       'pairs(&[("be", 2), ("to", 2), ("not", 1), ("or", 1)])'),
     T("invalid_utf8", "bytes [0x66, 0xFF]", "word_counts(&[0x66u8, 0xFF][..]).map_err(|e| e.kind())", "Err(io::ErrorKind::InvalidData)")],
    [READERS,
     T("empty", '""', 'word_counts("".as_bytes()).unwrap()', "Vec::<(String, usize)>::new()"),
     T("only_whitespace", '" \\n\\t "', 'word_counts(" \\n\\t ".as_bytes()).unwrap()', "Vec::<(String, usize)>::new()"),
     T("read_error_is_passed_on", "a reader that fails after some data", 'word_counts(Broken { data: b"a b" }).map_err(|e| e.kind())', "Err(io::ErrorKind::ConnectionReset)"),
     T("case_sensitive", '"Go go GO go"', 'word_counts("Go go GO go".as_bytes()).unwrap()', 'pairs(&[("go", 2), ("GO", 1), ("Go", 1)])'),
     T("unicode_whitespace", '"a\\u{3000}b\\u{a0}a"', 'word_counts("a\\u{3000}b\\u{a0}a".as_bytes()).unwrap()', 'pairs(&[("a", 2), ("b", 1)])'),
     T("split_char_across_reads", '"日本 日本", 2 bytes per read', 'word_counts(Trickle { data: "日本 日本".as_bytes(), step: 2 }).unwrap()', 'pairs(&[("日本", 2)])'),
     T("by_mut_reference", "&mut Cursor, then the cursor is at the end", "(word_counts(&mut c).unwrap(), c.position())", '(pairs(&[("q", 1)]), 1)',
       setup='let mut c = io::Cursor::new("q");'),
     T("dyn_worker_on_a_trickle", "word_counts_dyn over a 1-byte Trickle", 'word_counts_dyn(&mut Trickle { data: b"z z y", step: 1 }).unwrap()', 'pairs(&[("z", 2), ("y", 1)])'),
     T("chain_of_readers", '"ab c".chain(" ab")', 'word_counts("ab c".as_bytes().chain(" ab".as_bytes())).unwrap()', 'pairs(&[("ab", 2), ("c", 1)])'),
     T("invalid_utf8_mid_stream", "valid words then a lone continuation byte", "word_counts(Trickle { data: &bytes, step: 2 }).map_err(|e| e.kind())", "Err(io::ErrorKind::InvalidData)",
       setup='let mut bytes = b"ok ok ".to_vec();\nbytes.push(0x80);'),
     """
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4518);
         for _ in 0..300 {
             let n = rng.below(20);
             let text = rng.string(n, "ab é\\n");
             let step = 1 + rng.below(4);
             let mut want: Vec<(String, usize)> = Vec::new();
             for w in text.split_whitespace() {
                 match want.iter_mut().find(|(x, _)| x == w) {
                     Some((_, c)) => *c += 1,
                     None => want.push((w.to_string(), 1)),
                 }
             }
             want.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
             check!(format!("text = {text:?}, {step} bytes per read"), word_counts(Trickle { data: text.as_bytes(), step }).unwrap(), want);
         }
     }

     #[test]
     fn scale_many_words() {
         let text: String = (0..200_000).map(|i| format!("w{} ", i % 50_000)).collect();
         let got = word_counts(text.as_bytes()).unwrap();
         check!("200000 words, 50000 distinct", (got.len(), got[0].clone(), got.iter().all(|(_, n)| *n == 4)), (50_000, ("w0".to_string(), 4), true));
     }
     """],
    [("rust", "Signature to write: `pub fn word_counts_dyn(reader: &mut dyn Read) -> io::Result<Vec<(String, usize)>>`. The forwarder is `word_counts_dyn(&mut reader)`: `&mut R` coerces to `&mut dyn Read`."),
     ("rust", "Decoding each chunk separately fails when a multi-byte character is split between two reads. Collect the bytes first (`read_to_end`), then decode once with `String::from_utf8`."),
     ("rust", "`read_to_end` already retries `ErrorKind::Interrupted`; a hand-written `read` loop with `?` doesn't.")],
    ("""Every generic function is copied per type argument (monomorphization): fast calls, but code size and compile time grow with each reader type. std's pattern is a thin generic shell over a non-generic inner function, as in `fs::read<P: AsRef<Path>>(path: P) { fn inner(path: &Path) -> ... ; inner(path.as_ref()) }`. Here the inner function takes `&mut dyn Read`, trading one indirect call per `read` for a single compiled copy. Syntax to remember: `fn word_counts<R: Read>(mut reader: R)` (a generic parameter; `impl Read` in argument position is the same thing without a nameable `R`) vs `fn word_counts_dyn(reader: &mut dyn Read)` (one function, dynamic dispatch).""", "O(n + k log k) for n bytes and k distinct words", "O(n)"),
    "When is the generic version the better choice, despite the code size? What does `impl Read` in argument position change for callers (hint: turbofish)?",
    ["Monomorphization cost and the non-generic inner function trick.", "`&mut dyn Read` vs `R: Read`.", "Readers can return short reads and `Interrupted`."],
    rules=dict(unsafe=True),
    related=("L5", "S9", "Y4"),
    wrong=dict(
        ties_reversed=counts(order="b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0))"),
        manual_loop_no_retry=counts(read_body="""    let mut bytes = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;"""),
        lossy_decoding=counts(read_body="""    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();"""),
        ascii_whitespace_only=counts(split="text.split_ascii_whitespace()"),
    ),
))


def nonempty(pop_guard="self.tail.pop()", max_body="self.iter().fold(&self.head, |best, x| if x > best { x } else { best })", sort_body=None, try_from_body=None):
    sort_body = sort_body or """        // Put everything in one Vec, sort, and split the smallest back out as the head.
        let mut all = std::mem::take(&mut self.tail);
        all.sort();
        let mut rest = all.into_iter();
        if let Some(first) = rest.next() {
            if first < self.head {
                let old_head = std::mem::replace(&mut self.head, first);
                let mut tail: Vec<T> = rest.collect();
                let at = tail.partition_point(|x| *x <= old_head);
                tail.insert(at, old_head);
                self.tail = tail;
            } else {
                self.tail = std::iter::once(first).chain(rest).collect();
            }
        }"""
    try_from_body = try_from_body or """        let mut items = v.into_iter();
        match items.next() {
            Some(head) => Ok(NonEmpty { head, tail: items.collect() }),
            None => Err(Empty),
        }"""
    return f"""
/// A list that can't be empty: the first item is a separate field, so there's no empty value to represent.
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmpty<T> {{
    head: T,
    tail: Vec<T>,
}}

/// The error from converting an empty Vec.
#[derive(Debug, PartialEq)]
pub struct Empty;

impl<T> NonEmpty<T> {{
    pub fn new(head: T) -> Self {{
        NonEmpty {{ head, tail: Vec::new() }}
    }}

    pub fn push(&mut self, x: T) {{
        self.tail.push(x);
    }}

    /// Removes and returns the last item, unless it's the only one.
    pub fn pop(&mut self) -> Option<T> {{
        {pop_guard}
    }}

    pub fn first(&self) -> &T {{
        &self.head
    }}

    pub fn last(&self) -> &T {{
        self.tail.last().unwrap_or(&self.head)
    }}

    pub fn len(&self) -> usize {{
        1 + self.tail.len()
    }}

    pub fn get(&self, i: usize) -> Option<&T> {{
        if i == 0 {{ Some(&self.head) }} else {{ self.tail.get(i - 1) }}
    }}

    pub fn split_first(&self) -> (&T, &[T]) {{
        (&self.head, &self.tail)
    }}

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {{
        std::iter::once(&self.head).chain(&self.tail)
    }}

    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> NonEmpty<U> {{
        let head = f(self.head);
        NonEmpty {{ head, tail: self.tail.into_iter().map(f).collect() }}
    }}

    /// The largest item; the first of equal largest ones. No Option: there's always one.
    pub fn max(&self) -> &T
    where
        T: Ord,
    {{
        {max_body}
    }}

    pub fn sort(&mut self)
    where
        T: Ord,
    {{
{sort_body}
    }}

    pub fn into_vec(self) -> Vec<T> {{
        let mut v = Vec::with_capacity(self.len());
        v.push(self.head);
        v.extend(self.tail);
        v
    }}
}}

impl<T> TryFrom<Vec<T>> for NonEmpty<T> {{
    type Error = Empty;

    fn try_from(v: Vec<T>) -> Result<Self, Empty> {{
{try_from_body}
    }}
}}

impl<T> IntoIterator for NonEmpty<T> {{
    type Item = T;
    type IntoIter = std::iter::Chain<std::iter::Once<T>, std::vec::IntoIter<T>>;

    fn into_iter(self) -> Self::IntoIter {{
        std::iter::once(self.head).chain(self.tail)
    }}
}}
"""


NONEMPTY_STARTER = """
use std::marker::PhantomData;

/// A list that can't be empty.
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmpty<T> {
    // Replace this with a representation in which "empty" can't be expressed.
    _todo: PhantomData<T>,
}

/// The error from converting an empty Vec.
#[derive(Debug, PartialEq)]
pub struct Empty;

impl<T> NonEmpty<T> {
    pub fn new(head: T) -> Self {
        todo!()
    }

    pub fn push(&mut self, x: T) {
        todo!()
    }

    /// Removes and returns the last item, unless it's the only one.
    pub fn pop(&mut self) -> Option<T> {
        todo!()
    }

    pub fn first(&self) -> &T {
        todo!()
    }

    pub fn last(&self) -> &T {
        todo!()
    }

    pub fn len(&self) -> usize {
        todo!()
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        todo!()
    }

    pub fn split_first(&self) -> (&T, &[T]) {
        todo!()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        // Placeholder so the crate compiles.
        std::iter::empty()
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> NonEmpty<U> {
        todo!()
    }

    /// The largest item; the first of equal largest ones. No Option: there's always one.
    pub fn max(&self) -> &T
    where
        T: Ord,
    {
        todo!()
    }

    pub fn sort(&mut self)
    where
        T: Ord,
    {
        todo!()
    }

    pub fn into_vec(self) -> Vec<T> {
        todo!()
    }
}

impl<T> TryFrom<Vec<T>> for NonEmpty<T> {
    type Error = Empty;

    fn try_from(v: Vec<T>) -> Result<Self, Empty> {
        todo!()
    }
}

impl<T> IntoIterator for NonEmpty<T> {
    type Item = T;
    // Change this to whatever iterator your representation gives.
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        todo!()
    }
}
"""

NE_HELPERS = PROBES + """
fn ne(v: Vec<i32>) -> NonEmpty<i32> {
    NonEmpty::try_from(v).unwrap()
}
"""

P.append(write(
    "invalid-states-unrepresentable", "An API that makes invalid states unrepresentable", "hard", "type-level-design", ["make invalid states unrepresentable", "NonEmpty", "API design"],
    """
        Build `NonEmpty<T>`, a list with at least one item, so that "empty" can't be expressed at all, and let the
        API say so: `first`, `last` and `max` return `&T`, not `Option<&T>`.

        - `pop` removes the last item but never the only one (it returns `None` instead).
        - `max` returns the first of equal largest items; `sort` sorts ascending (equal items may be reordered).
        - `NonEmpty::try_from(vec)` fails with `Empty` for an empty `Vec`; `into_vec` and `into_iter` give the
          items back in order; `map` keeps the length.
        - There's no `Default`: a default `NonEmpty` would be empty.
    """,
    NONEMPTY_STARTER,
    nonempty(),
    [NE_HELPERS,
     T("first_last_max_are_not_options", "[3, 9, 2]: first, last, max", "{ let v = ne(vec![3, 9, 2]); let (a, b, c): (&i32, &i32, &i32) = (v.first(), v.last(), v.max()); (*a, *b, *c) }", "(3, 2, 9)"),
     T("pop_never_empties", "[1, 2]: pop three times", "{ let mut v = ne(vec![1, 2]); (v.pop(), v.pop(), v.pop(), v.len(), *v.first()) }", "(Some(2), None, None, 1, 1)"),
     T("try_from_empty", "an empty Vec", "NonEmpty::<i32>::try_from(Vec::new())", "Err(Empty)"),
     T("round_trip", "vec [4, 5, 6] -> NonEmpty -> map(x * 10) -> into_vec", "ne(vec![4, 5, 6]).map(|x| x * 10).into_vec()", "vec![40, 50, 60]"),
     T("no_default", "is NonEmpty<i32> Default?", "Probe::<NonEmpty<i32>>::DEFAULT", "false")],
    [NE_HELPERS,
     T("single", "NonEmpty::new(7)", "{ let v = NonEmpty::new(7); (*v.first(), *v.last(), *v.max(), v.len()) }", "(7, 7, 7, 1)"),
     T("get", "[1, 2, 3]: get 0, 2, 3", "{ let v = ne(vec![1, 2, 3]); (v.get(0).copied(), v.get(2).copied(), v.get(3).copied()) }", "(Some(1), Some(3), None)"),
     T("max_first_of_ties", "[5, 5, 4]: which 5 is the max?", "{ let v = ne(vec![5, 5, 4]); std::ptr::eq(v.max(), v.first()) }", "true"),
     T("sort_moves_a_smaller_item_to_the_front", "[5, 3, 9, 1] sorted", "{ let mut v = ne(vec![5, 3, 9, 1]); v.sort(); v.into_vec() }", "vec![1, 3, 5, 9]"),
     T("sort_keeps_the_front_when_smallest", "[1, 3, 2] sorted", "{ let mut v = ne(vec![1, 3, 2]); v.sort(); v.into_vec() }", "vec![1, 2, 3]"),
     T("sort_single", "[7] sorted", "{ let mut v = NonEmpty::new(7); v.sort(); v.into_vec() }", "vec![7]"),
     T("into_iter_order", "strings [a, b, c] by value", 'NonEmpty::try_from(vec!["a".to_string(), "b".to_string(), "c".to_string()]).unwrap().into_iter().collect::<Vec<_>>().join("")', '"abc"'),
     T("iter_and_split_first", "[1, 2, 3]: iter, split_first", "{ let v = ne(vec![1, 2, 3]); (v.iter().copied().collect::<Vec<_>>(), v.split_first().0.clone(), v.split_first().1.to_vec()) }", "(vec![1, 2, 3], 1, vec![2, 3])"),
     T("map_changes_type", "[1, 22] map to_string", "ne(vec![1, 22]).map(|x| x.to_string()).into_vec()", 'vec!["1".to_string(), "22".to_string()]'),
     T("push_after_pops", "[1]: pop, push 2, last", "{ let mut v = NonEmpty::new(1); let p = v.pop(); v.push(2); (p, *v.last(), v.len()) }", "(None, 2, 2)"),
     T("try_from_non_empty_is_ok", "vec [0]", "NonEmpty::try_from(vec![0]).map(|v| v.len())", "Ok(1)"),
     """
     #[test]
     fn random_vs_vec_model() {
         let mut rng = anneal_prelude::Rng::new(4519);
         for _ in 0..300 {
             let first = rng.int(-9, 9);
             let mut v = NonEmpty::new(first);
             let mut model = vec![first];
             let mut log = vec![format!("new({first})")];
             for _ in 0..rng.below(15) {
                 match rng.below(4) {
                     0 | 1 => {
                         let x = rng.int(-9, 9);
                         log.push(format!("push({x})"));
                         v.push(x);
                         model.push(x);
                     }
                     2 => {
                         log.push("pop".to_string());
                         let want = if model.len() > 1 { model.pop() } else { None };
                         check!(log.join(", "), v.pop(), want);
                     }
                     _ => {
                         log.push("sort".to_string());
                         v.sort();
                         model.sort();
                     }
                 }
                 let mut max_at = 0;
                 for i in 1..model.len() {
                     if model[i] > model[max_at] {
                         max_at = i;
                     }
                 }
                 let i = rng.below(model.len() + 1);
                 check!(log.join(", "), (v.len(), *v.first(), *v.last(), *v.max(), v.get(i).copied(), v.iter().copied().collect::<Vec<_>>()),
                        (model.len(), model[0], model[model.len() - 1], model[max_at], model.get(i).copied(), model.clone()));
             }
             check!(log.join(", "), v.into_vec(), model);
         }
     }
     """],
    [("approach", "Represent it as `head: T` plus `tail: Vec<T>`. There is no value of that struct with zero items, so `first` is just `&self.head`, and `pop` only pops from `tail`."),
     ("rust", "`IntoIterator` by value: `type IntoIter = Chain<Once<T>, vec::IntoIter<T>>`, built with `std::iter::once(self.head).chain(self.tail)`."),
     ("edge", "`sort` with a separate head: sort the tail, and if its smallest item is smaller than the head, swap it in and put the old head where it belongs.")],
    ("""Make the representation carry the invariant, and the checks disappear from the API: with `head + tail`, `first()` can't fail, so it returns `&T`, and every caller loses an `unwrap`. Checks happen once, at the boundary (`TryFrom<Vec<T>>`). `Vec<T>` plus a runtime check would pass the same tests, but then every method re-proves the invariant and a future `pub fn clear` could break it. Syntax to remember: `impl<T> TryFrom<Vec<T>> for NonEmpty<T> { type Error = Empty; fn try_from(v: Vec<T>) -> Result<Self, Empty> }`, and `pub fn map<U>(self, f: impl FnMut(T) -> U) -> NonEmpty<U>` (a generic method on a generic type).""", "O(1) push/pop/first/last; O(n) max; O(n log n) sort", "O(n)"),
    "Where else would you use this pattern in a backend? Give two types whose invalid states you'd remove from the representation, and what you'd do at the boundary.",
    ["Encode invariants in the representation, not in runtime checks.", "Boundary conversions with `TryFrom` and a precise error type.", "`IntoIterator` with a named `IntoIter` type."],
    related=("L5", "L7", "M1"),
    wrong=dict(
        max_returns_last_tie=nonempty(max_body="self.iter().max().unwrap()"),
        sort_ignores_head=nonempty(sort_body="        self.tail.sort();"),
        derives_default=nonempty().replace("#[derive(Debug, Clone, PartialEq)]\npub struct NonEmpty<T> {", "#[derive(Debug, Clone, PartialEq, Default)]\npub struct NonEmpty<T> {", 1),
    ),
))

STAGES = [
    ("generic-code", "Generic code", "easy"),
    ("bounds-associated-types", "Bounds & associated types", "medium"),
    ("compile-time-rust", "Compile-time Rust", "medium"),
    ("type-level-design", "Type-level design", "hard"),
]

# `source` and `examples` are optional; drop empty ones so problem.toml stays tidy.
for p in P:
    if not p.get("source"):
        p.pop("source", None)
    if p.get("rules") is None:
        p.pop("rules", None)
    if p.get("wrong") is None:
        p.pop("wrong", None)

# Review of 2026-11-28 (group B). The folders were edited by hand; these lines bring the same edits
# back so regenerating reproduces them. FORMATTED holds the starter and solution as rustfmt left them
# (commit 37f80727) for the problems the review touched; the rest are the tests, wrong solutions and
# [perf] keys the review added.
FORMATTED = {
    'fix-const-generic-traps': (
        r'''
/// A fixed-capacity ring buffer. Pushing onto a full ring evicts and returns the oldest item.
pub struct Ring<T, const N: usize> {
    items: [T; N],
    start: usize,
    len: usize,
}

impl<T: Default, const N: usize> Ring<T, N> {
    pub fn new() -> Self {
        Ring {
            items: [T::default(); N],
            start: 0,
            len: 0,
        }
    }
}

impl<T, const N: usize> Ring<T, N> {
    pub const CAPACITY: usize = N;

    /// Adds `x` as the newest item; if the ring was full, returns the evicted oldest one.
    pub fn push(&mut self, x: T) -> Option<T> {
        if self.len == N {
            let old = std::mem::replace(&mut self.items[self.start], x);
            self.start = (self.start + 1) % N;
            Some(old)
        } else {
            self.items[(self.start + self.len) % N] = x;
            self.len += 1;
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).map(move |i| &self.items[(self.start + i) % N])
    }
}

/// `a` followed by `b`.
pub fn concat<T: Copy + Default, const A: usize, const B: usize>(
    a: [T; A],
    b: [T; B],
) -> [T; A + B] {
    let mut out = [T::default(); A + B];
    out[..A].copy_from_slice(&a);
    out[A..].copy_from_slice(&b);
    out
}
''',
        r'''
/// A fixed-capacity ring buffer. Pushing onto a full ring evicts and returns the oldest item.
pub struct Ring<T, const N: usize> {
    // Option<T> slots: an empty ring needs no T at all, so no `T: Default` and no `T: Copy`.
    items: [Option<T>; N],
    start: usize,
    len: usize,
}

impl<T, const N: usize> Ring<T, N> {
    pub const CAPACITY: usize = N;

    pub fn new() -> Self {
        // `[None; N]` would need `Option<T>: Copy`; from_fn builds each slot separately.
        Ring {
            items: std::array::from_fn(|_| None),
            start: 0,
            len: 0,
        }
    }

    /// Adds `x` as the newest item; if the ring was full, returns the evicted oldest one.
    pub fn push(&mut self, x: T) -> Option<T> {
        if N == 0 {
            return Some(x);
        }
        if self.len == N {
            let old = self.items[self.start].replace(x);
            self.start = (self.start + 1) % N;
            old
        } else {
            self.items[(self.start + self.len) % N] = Some(x);
            self.len += 1;
            None
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len).map(move |i| self.items[(self.start + i) % N].as_ref().unwrap())
    }
}

/// `a` followed by `b`. `[T; A + B]` isn't allowed on stable, so the caller's length `C` is checked at compile time.
pub fn concat<T, const A: usize, const B: usize, const C: usize>(a: [T; A], b: [T; B]) -> [T; C] {
    const { assert!(A + B == C, "concat: C must be A + B") };
    let mut items = a.into_iter().chain(b);
    std::array::from_fn(|_| items.next().unwrap())
}
''',
    ),
    'zero-sized-strategies': (
        r'''
use std::cmp::Ordering;
use std::marker::PhantomData;

/// How to order items. Implementors are zero-sized: the strategy lives in the type, not in the value.
pub trait Order {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering;
}

pub struct Asc;
pub struct Desc;

impl Order for Asc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        todo!()
    }
}

impl Order for Desc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        todo!()
    }
}

/// Items kept sorted by `O`, ascending unless you say otherwise.
pub struct SortedVec<T, O = Asc> {
    items: Vec<T>,
    order: PhantomData<O>,
}

impl<T: Ord, O: Order> SortedVec<T, O> {
    pub fn new() -> Self {
        todo!()
    }

    /// Inserts `x` after any items that compare equal to it.
    pub fn insert(&mut self, x: T) {
        todo!()
    }

    /// Whether some item compares equal to `x` under `O`.
    pub fn contains(&self, x: &T) -> bool {
        todo!()
    }

    pub fn as_slice(&self) -> &[T] {
        todo!()
    }

    /// The same items sorted by another order; equal items keep their relative order.
    pub fn reorder<O2: Order>(self) -> SortedVec<T, O2> {
        todo!()
    }
}

impl<T: Ord, O: Order> FromIterator<T> for SortedVec<T, O> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        todo!()
    }
}
''',
        r'''
use std::cmp::Ordering;
use std::marker::PhantomData;

/// How to order items. Implementors are zero-sized: the strategy lives in the type, not in the value.
pub trait Order {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering;
}

pub struct Asc;
pub struct Desc;

impl Order for Asc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        a.cmp(b)
    }
}

impl Order for Desc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        b.cmp(a)
    }
}

/// Items kept sorted by `O`, ascending unless you say otherwise.
pub struct SortedVec<T, O = Asc> {
    items: Vec<T>,
    order: PhantomData<O>,
}

impl<T: Ord, O: Order> SortedVec<T, O> {
    pub fn new() -> Self {
        SortedVec {
            items: Vec::new(),
            order: PhantomData,
        }
    }

    /// Inserts `x` after any items that compare equal to it.
    pub fn insert(&mut self, x: T) {
        let i = self
            .items
            .partition_point(|y| O::cmp(y, &x) != Ordering::Greater);
        self.items.insert(i, x);
    }

    /// Whether some item compares equal to `x` under `O`.
    pub fn contains(&self, x: &T) -> bool {
        self.items.binary_search_by(|y| O::cmp(y, x)).is_ok()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// The same items sorted by another order; equal items keep their relative order.
    pub fn reorder<O2: Order>(mut self) -> SortedVec<T, O2> {
        self.items.sort_by(O2::cmp);
        SortedVec {
            items: self.items,
            order: PhantomData,
        }
    }
}

impl<T: Ord, O: Order> FromIterator<T> for SortedVec<T, O> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = SortedVec::new();
        for x in iter {
            v.insert(x);
        }
        v
    }
}
''',
    ),
}
ADDED_HIDDEN = {
    'zero-sized-strategies': [r'''
std::thread_local! {
    static COMPARISONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Ascending, and counts every comparison the SortedVec asks for.
struct Counting;

impl Order for Counting {
    fn cmp<T: Ord>(a: &T, b: &T) -> std::cmp::Ordering {
        COMPARISONS.with(|c| c.set(c.get() + 1));
        a.cmp(b)
    }
}

#[test]
fn comparisons_grow_with_log_n() {
    let mut rng = anneal_prelude::Rng::new(4520);
    let n = 2048u64;
    let xs: Vec<i64> = (0..n).map(|_| rng.int(0, 1_000_000)).collect();
    COMPARISONS.with(|c| c.set(0));
    let v: SortedVec<i64, Counting> = xs.iter().copied().collect();
    let inserts = COMPARISONS.with(|c| c.replace(0));
    let found = xs.iter().filter(|x| v.contains(x)).count() as u64;
    let lookups = COMPARISONS.with(|c| c.get());
    check!(
        format!("2048 random inserts then 2048 lookups: {} and {} comparisons per call, at most 40 (log2 2048 is 11)", inserts / n, lookups / n),
        (found, inserts <= 40 * n, lookups <= 40 * n),
        (n, true, true)
    );
}
'''],
}
ADDED_WRONG = {
    'zero-sized-strategies': {
        'linear_insert_from_the_back': r'''
use std::cmp::Ordering;
use std::marker::PhantomData;

/// How to order items. Implementors are zero-sized: the strategy lives in the type, not in the value.
pub trait Order {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering;
}

pub struct Asc;
pub struct Desc;

impl Order for Asc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        a.cmp(b)
    }
}

impl Order for Desc {
    fn cmp<T: Ord>(a: &T, b: &T) -> Ordering {
        b.cmp(a)
    }
}

/// Items kept sorted by `O`, ascending unless you say otherwise.
pub struct SortedVec<T, O = Asc> {
    items: Vec<T>,
    order: PhantomData<O>,
}

impl<T: Ord, O: Order> SortedVec<T, O> {
    pub fn new() -> Self {
        SortedVec {
            items: Vec::new(),
            order: PhantomData,
        }
    }

    /// Inserts `x` after any items that compare equal to it.
    pub fn insert(&mut self, x: T) {
        // Scan back from the end: fast for input that arrives in order, but O(n) comparisons in general.
        let i = self
            .items
            .iter()
            .rposition(|y| O::cmp(y, &x) != Ordering::Greater)
            .map_or(0, |i| i + 1);
        self.items.insert(i, x);
    }

    /// Whether some item compares equal to `x` under `O`.
    pub fn contains(&self, x: &T) -> bool {
        self.items.binary_search_by(|y| O::cmp(y, x)).is_ok()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// The same items sorted by another order; equal items keep their relative order.
    pub fn reorder<O2: Order>(mut self) -> SortedVec<T, O2> {
        self.items.sort_by(O2::cmp);
        SortedVec {
            items: self.items,
            order: PhantomData,
        }
    }
}

impl<T: Ord, O: Order> FromIterator<T> for SortedVec<T, O> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = SortedVec::new();
        for x in iter {
            v.insert(x);
        }
        v
    }
}
''',
    },
}
ADDED_PERF = {}
for p in P:
    s = p["slug"]
    if s in FORMATTED:
        p["starter"], p["solution"] = FORMATTED[s]
    if s in ADDED_HIDDEN:
        p["hidden"] = list(p["hidden"]) + ADDED_HIDDEN[s]
    if s in ADDED_WRONG:
        p["wrong"] = {**(p.get("wrong") or {}), **ADDED_WRONG[s]}
    if s in ADDED_PERF:
        p["perf"] = ADDED_PERF[s]


if __name__ == "__main__":
    n = write_track("l5-generics", "L5", "Generics & associated types", "L", "core", 5,
                    "Generic code is written once and compiled per type. Bounds say what a type must do; associated types, const generics and marker types move rules into the type checker.",
                    STAGES, P)
    print("L5", n)
