from author import T, write_track

P = []


def fix(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, rules=None, related=("L5",), wrong=None):
    return dict(slug=slug, title=title, mode="fix", level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, rules=rules, related=list(related), wrong=wrong)


def write(slug, title, level, stage, tags, statement, starter, solution, visible, hidden, hints, notes, follow_up, teaches, related=("L5",), source=None, examples=(), wrong=None):
    return dict(slug=slug, title=title, level=level, stage=stage, tags=tags, statement=statement, starter=starter,
                solution=solution, visible=visible, hidden=hidden, hints=hints, notes=notes, follow_up=follow_up,
                teaches=teaches, related=list(related), source=source, examples=list(examples), wrong=wrong)


def sub(s, old, new):
    """str.replace that fails loudly when `old` isn't there (a wrong solution that silently equals the reference)."""
    assert old in s, f"not found: {old[:60]!r}"
    return s.replace(old, new)


# ---------------------------------------------------------------- define & implement (easy)
# The easy band is recall: the starter is the code that *uses* the traits, and the solver writes the trait layer
# from memory. Those starters can't compile until it's written, so these problems use fix mode.

SHAPE_HEAD = r"""
use std::f64::consts::PI;

pub struct Circle {
    pub r: f64,
}

pub struct Rect {
    pub w: f64,
    pub h: f64,
}

/// Another trait with a `name` method. Don't change it.
pub trait Label {
    fn name(&self) -> String;
}

impl Label for Rect {
    fn name(&self) -> String {
        "box".to_string()
    }
}
"""

SHAPE_TRAITS = r"""
pub trait Named {
    fn name(&self) -> String;
}

pub trait Shape: Named {
    /// Number of straight sides (0 for a circle).
    const SIDES: u32;

    fn area(&self) -> f64;
    fn perimeter(&self) -> f64;

    /// "<name>, <SIDES> sides, area <area to 2 decimals>".
    fn describe(&self) -> String {
        format!("{}, {} sides, area {:.2}", self.name(), Self::SIDES, self.area())
    }
}

impl Named for Circle {
    fn name(&self) -> String {
        "circle".to_string()
    }
}

impl Shape for Circle {
    const SIDES: u32 = 0;

    fn area(&self) -> f64 {
        PI * self.r * self.r
    }

    fn perimeter(&self) -> f64 {
        2.0 * PI * self.r
    }
}

impl Named for Rect {
    fn name(&self) -> String {
        "rect".to_string()
    }
}

impl Shape for Rect {
    const SIDES: u32 = 4;

    fn area(&self) -> f64 {
        self.w * self.h
    }

    fn perimeter(&self) -> f64 {
        2.0 * (self.w + self.h)
    }
}
"""

SHAPE_TAIL = r"""
/// Total number of sides across the shapes.
pub fn total_sides<S: Shape>(shapes: &[S]) -> u32 {
    S::SIDES * shapes.len() as u32
}

/// "<Named name>/<Label name>" for a rect: "rect/box".
pub fn both_names(r: &Rect) -> String {
    BODY
}
"""

SHAPE_SOLUTION = SHAPE_HEAD + SHAPE_TRAITS + SHAPE_TAIL.replace("BODY", 'format!("{}/{}", Named::name(r), <Rect as Label>::name(r))')

P.append(fix(
    "shape-trait", "A Shape trait", "easy", "define-implement", ["supertraits", "associated consts", "default methods", "fully qualified syntax"],
    """
        The code at the bottom uses two traits that don't exist yet. Write them and implement both for `Circle`
        and `Rect`:

        - `Named`: `fn name(&self) -> String`, giving `"circle"` and `"rect"`.
        - `Shape`, which requires `Named`: an associated const `SIDES: u32` (0 for a circle, 4 for a rect),
          `area` and `perimeter` returning `f64`, and a default `describe` returning
          `"<name>, <SIDES> sides, area <area to 2 decimals>"`.

        Then finish `both_names`. `Rect` also implements `Label`, which has its own `name`.
    """,
    SHAPE_HEAD + "\n// TODO: the Named and Shape traits, and their impls for Circle and Rect.\n" + SHAPE_TAIL.replace("BODY", "todo!()"),
    SHAPE_SOLUTION,
    [T("describe_circle", "Circle { r: 1.0 }.describe()", "Circle { r: 1.0 }.describe()", '"circle, 0 sides, area 3.14"'),
     T("describe_uses_named", "Rect { w: 2.0, h: 3.0 }.describe()", "Rect { w: 2.0, h: 3.0 }.describe()", '"rect, 4 sides, area 6.00"'),
     T("sides_const", "<Rect as Shape>::SIDES, Circle::SIDES", "(<Rect as Shape>::SIDES, Circle::SIDES)", "(4, 0)"),
     T("both_names_rect", "both_names(&Rect { w: 1.0, h: 1.0 })", "both_names(&Rect { w: 1.0, h: 1.0 })", '"rect/box"'),
     """
     #[test]
     fn a_new_shape() {
         // Implemented outside your crate: it only needs Named, the const, area and perimeter.
         struct Tri(f64);
         impl Named for Tri {
             fn name(&self) -> String {
                 "tri".to_string()
             }
         }
         impl Shape for Tri {
             const SIDES: u32 = 3;
             fn area(&self) -> f64 {
                 self.0 * self.0 * 3f64.sqrt() / 4.0
             }
             fn perimeter(&self) -> f64 {
                 3.0 * self.0
             }
         }
         check!("Tri(2.0).describe()", Tri(2.0).describe(), "tri, 3 sides, area 1.73");
         check!("total_sides(&[Tri(1.0), Tri(2.0)])", total_sides(&[Tri(1.0), Tri(2.0)]), 6);
     }
     """],
    [T("circle_perimeter", "Circle { r: 2.5 }.perimeter(), 4 decimals", 'format!("{:.4}", Circle { r: 2.5 }.perimeter())', '"15.7080"'),
     T("rect_perimeter", "Rect { w: 2.0, h: 3.5 }.perimeter()", "Rect { w: 2.0, h: 3.5 }.perimeter()", "11.0"),
     T("describe_rounds", "Circle { r: 2.0 }.describe()", "Circle { r: 2.0 }.describe()", '"circle, 0 sides, area 12.57"'),
     T("describe_zero_rect", "Rect { w: 0.0, h: 9.0 }.describe()", "Rect { w: 0.0, h: 9.0 }.describe()", '"rect, 4 sides, area 0.00"'),
     T("total_sides_empty", "total_sides::<Rect>(&[])", "total_sides::<Rect>(&[])", "0"),
     T("total_sides_rects", "three rects", "total_sides(&[Rect { w: 1.0, h: 1.0 }, Rect { w: 2.0, h: 1.0 }, Rect { w: 3.0, h: 1.0 }])", "12"),
     T("total_sides_circles", "two circles", "total_sides(&[Circle { r: 1.0 }, Circle { r: 2.0 }])", "0"),
     T("label_untouched", "<Rect as Label>::name", "<Rect as Label>::name(&Rect { w: 1.0, h: 1.0 })", '"box"'),
     T("named_on_circle", "Named::name(&Circle { r: 1.0 })", "Named::name(&Circle { r: 1.0 })", '"circle"'),
     """
     #[test]
     fn supertrait_gives_name() {
         // With only `S: Shape` in scope, `name` must come from the supertrait.
         fn via_shape<S: Shape>(s: &S) -> String {
             s.name()
         }
         check!("via_shape(&Rect { w: 1.0, h: 1.0 })", via_shape(&Rect { w: 1.0, h: 1.0 }), "rect");
         check!("via_shape(&Circle { r: 1.0 })", via_shape(&Circle { r: 1.0 }), "circle");
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4401);
         for _ in 0..300 {
             let (w, h) = (rng.int(0, 40) as f64 / 4.0, rng.int(0, 40) as f64 / 4.0);
             let r = Rect { w, h };
             check!(format!("Rect {{ w: {w}, h: {h} }}.describe()"), r.describe(), format!("rect, 4 sides, area {:.2}", w * h));
             check!(format!("Rect {{ w: {w}, h: {h} }}.perimeter()"), r.perimeter(), 2.0 * (w + h));
             let rad = rng.int(0, 40) as f64 / 8.0;
             check!(format!("Circle {{ r: {rad} }}.describe()"), Circle { r: rad }.describe(), format!("circle, 0 sides, area {:.2}", std::f64::consts::PI * rad * rad));
         }
     }
     """],
    [("rust", "A supertrait goes after a colon: `trait Shape: Named { ... }`. An associated const is declared `const SIDES: u32;` in the trait and given a value in each impl."),
     ("rust", "Inside `describe`, read the const through `Self::SIDES`. In `both_names`, `r.name()` is ambiguous because `Rect` has two `name` methods; name the trait: `Named::name(r)` or `<Rect as Label>::name(r)`.")],
    ("""A supertrait `Shape: Named` means every `Shape` is also `Named`, so `Shape`'s defaults (and any `S: Shape` code) can call `name`. An associated const is per type, read as `Self::SIDES` or `<Rect as Shape>::SIDES`. When two traits give a type methods with the same name, call one with fully qualified syntax.

Syntax: `trait Shape: Named { const SIDES: u32; fn describe(&self) -> String { ... } }` · `impl Shape for Rect { const SIDES: u32 = 4; ... }` · `<Rect as Label>::name(r)`.

A trait with an associated const is not dyn-compatible: `dyn Shape` has no single value for `SIDES`.""", "O(1)", "O(1)"),
    "Why can't `Vec<Box<dyn Shape>>` exist here, and how would you change `Shape` so it can?",
    ["Supertraits: `trait Shape: Named`.", "Associated consts are declared in the trait and set per impl.", "Fully qualified syntax picks between same-named methods."],
    related=("L5", "S8"),
    wrong=dict(
        both_names_reversed=SHAPE_HEAD + SHAPE_TRAITS + SHAPE_TAIL.replace("BODY", 'format!("{}/{}", <Rect as Label>::name(r), Named::name(r))'),
        circle_one_side=SHAPE_HEAD + SHAPE_TRAITS.replace("const SIDES: u32 = 0;", "const SIDES: u32 = 1;") + SHAPE_TAIL.replace("BODY", 'format!("{}/{}", Named::name(r), <Rect as Label>::name(r))'),
    ),
))

METRIC_HEAD = r"""
pub struct Latency {
    pub samples: Vec<f64>,
}

/// Requests per second, one value per second. Idle seconds (0.0) don't count toward the mean.
pub struct Throughput {
    pub rps: Vec<f64>,
}
"""

METRIC_TRAIT = r"""
pub trait Metric {
    fn name(&self) -> &str;
    fn values(&self) -> &[f64];

    /// The mean of the values, or None when there are none.
    fn mean(&self) -> Option<f64> {
        let v = self.values();
        if v.is_empty() {
            None
        } else {
            Some(v.iter().sum::<f64>() / v.len() as f64)
        }
    }

    /// "<name>: n=<number of values> mean=<mean, 2 decimals>", or "<name>: no data" when `mean` is None.
    fn report(&self) -> String {
        match self.mean() {
            Some(m) => format!("{}: n={} mean={:.2}", self.name(), self.values().len(), m),
            None => format!("{}: no data", self.name()),
        }
    }

    /// How many values `pred` accepts.
    fn count_where<F: Fn(f64) -> bool>(&self, pred: F) -> usize
    where
        Self: Sized,
    {
        self.values().iter().filter(|&&v| pred(v)).count()
    }
}

impl Metric for Latency {
    fn name(&self) -> &str {
        "latency"
    }

    fn values(&self) -> &[f64] {
        &self.samples
    }
}

impl Metric for Throughput {
    fn name(&self) -> &str {
        "throughput"
    }

    fn values(&self) -> &[f64] {
        &self.rps
    }

    fn mean(&self) -> Option<f64> {
        let busy: Vec<f64> = self.rps.iter().copied().filter(|&v| v != 0.0).collect();
        if busy.is_empty() {
            None
        } else {
            Some(busy.iter().sum::<f64>() / busy.len() as f64)
        }
    }
}
"""

METRIC_TAIL = r"""
/// Every metric's report, in order.
pub fn reports(metrics: &[Box<dyn Metric>]) -> Vec<String> {
    metrics.iter().map(|m| m.report()).collect()
}
"""

P.append(fix(
    "default-methods", "Default methods", "easy", "define-implement", ["default methods", "generic methods", "where Self: Sized"],
    """
        Write the `Metric` trait that `reports` uses, and implement it for `Latency` (name `"latency"`) and
        `Throughput` (name `"throughput"`).

        - Required: `fn name(&self) -> &str` and `fn values(&self) -> &[f64]`.
        - Default `mean(&self) -> Option<f64>`: the mean of the values, `None` when there are none.
          `Throughput` overrides it to ignore idle (`0.0`) seconds.
        - Default `report(&self) -> String`: `"<name>: n=<number of values> mean=<mean, 2 decimals>"`, or
          `"<name>: no data"` when `mean` is `None`.
        - Default generic `count_where<F: Fn(f64) -> bool>(&self, pred: F) -> usize`.
    """,
    METRIC_HEAD + "\n// TODO: the Metric trait and its impls for Latency and Throughput.\n" + METRIC_TAIL,
    METRIC_HEAD + METRIC_TRAIT + METRIC_TAIL,
    [T("latency_report", "Latency { samples: [10, 20, 30] }.report()", "Latency { samples: vec![10.0, 20.0, 30.0] }.report()", '"latency: n=3 mean=20.00"'),
     T("no_data", "Latency { samples: [] }.report()", "Latency { samples: vec![] }.report()", '"latency: no data"'),
     T("report_uses_the_override", "Throughput { rps: [0, 100, 0, 200] }.report()", "Throughput { rps: vec![0.0, 100.0, 0.0, 200.0] }.report()", '"throughput: n=4 mean=150.00"'),
     T("reports_through_dyn", "reports([Latency [5], Throughput [0]])", "reports(&ms)", 'vec!["latency: n=1 mean=5.00", "throughput: no data"]',
       setup="let ms: Vec<Box<dyn Metric>> = vec![Box::new(Latency { samples: vec![5.0] }), Box::new(Throughput { rps: vec![0.0] })];"),
     T("count_where_closure", "Latency [120, 80, 300], count values > limit (100)", "Latency { samples: vec![120.0, 80.0, 300.0] }.count_where(|v| v > limit)", "2",
       setup="let limit = 100.0;"),
     """
     #[test]
     fn defaults_for_a_new_type() {
         struct Temps(Vec<f64>);
         impl Metric for Temps {
             fn name(&self) -> &str {
                 "temps"
             }
             fn values(&self) -> &[f64] {
                 &self.0
             }
         }
         check!("Temps([21.5, 22.5, 23.0, 25.0]).report()", Temps(vec![21.5, 22.5, 23.0, 25.0]).report(), "temps: n=4 mean=23.00");
     }
     """],
    [T("empty_mean", "Latency { samples: [] }.mean()", "Latency { samples: vec![] }.mean()", "None"),
     T("throughput_mean", "Throughput { rps: [0, 100, 0, 200] }.mean()", "Throughput { rps: vec![0.0, 100.0, 0.0, 200.0] }.mean()", "Some(150.0)"),
     T("throughput_all_idle", "Throughput { rps: [0, 0] }.report()", "Throughput { rps: vec![0.0, 0.0] }.report()", '"throughput: no data"'),
     T("throughput_empty", "Throughput { rps: [] }.mean()", "Throughput { rps: vec![] }.mean()", "None"),
     T("rounding", "Latency { samples: [1, 2, 2] }.report()", "Latency { samples: vec![1.0, 2.0, 2.0] }.report()", '"latency: n=3 mean=1.67"'),
     T("negatives", "Latency { samples: [-4, 1] }.mean()", "Latency { samples: vec![-4.0, 1.0] }.mean()", "Some(-1.5)"),
     T("count_where_counts_idle", "Throughput [0, 5, 0], count zeros", "Throughput { rps: vec![0.0, 5.0, 0.0] }.count_where(|v| v == 0.0)", "2"),
     T("count_where_none", "Latency [], anything", "Latency { samples: vec![] }.count_where(|_| true)", "0"),
     T("reports_empty", "reports(&[])", "reports(&[])", "Vec::<String>::new()"),
     """
     #[test]
     fn override_seen_through_dyn() {
         // An override of `mean` must change what the default `report` prints, even through `dyn Metric`.
         struct Median(Vec<f64>);
         impl Metric for Median {
             fn name(&self) -> &str {
                 "median"
             }
             fn values(&self) -> &[f64] {
                 &self.0
             }
             fn mean(&self) -> Option<f64> {
                 let mut v = self.0.clone();
                 v.sort_by(f64::total_cmp);
                 v.get(v.len() / 2).copied()
             }
         }
         let ms: Vec<Box<dyn Metric>> = vec![Box::new(Median(vec![1.0, 100.0, 3.0]))];
         check!("reports([Median [1, 100, 3]])", reports(&ms), vec!["median: n=3 mean=3.00".to_string()]);
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4402);
         for _ in 0..300 {
             let n = rng.below(6);
             let v: Vec<f64> = rng.vec::<i64>(n, 0, 3).into_iter().map(|x| x as f64 * 10.0).collect();
             let busy: Vec<f64> = v.iter().copied().filter(|&x| x != 0.0).collect();
             let want = if busy.is_empty() {
                 "throughput: no data".to_string()
             } else {
                 format!("throughput: n={} mean={:.2}", n, busy.iter().sum::<f64>() / busy.len() as f64)
             };
             check!(format!("Throughput {v:?}"), Throughput { rps: v.clone() }.report(), want);
             let want = if n == 0 { "latency: no data".to_string() } else { format!("latency: n={} mean={:.2}", n, v.iter().sum::<f64>() / n as f64) };
             check!(format!("Latency {v:?}"), Latency { samples: v.clone() }.report(), want);
         }
     }
     """],
    [("approach", "`report` must call `self.mean()`, not recompute the mean, or `Throughput`'s override never shows."),
     ("rust", "A generic method can't go in a vtable, so `dyn Metric` is rejected (E0038) while `count_where` is generic. Add `where Self: Sized` to that one method: it stays callable on concrete types and is left out of `dyn Metric`.")],
    ("""Default methods are written once in the trait and may call the required ones; an impl can override any of them, and the other defaults then call the override (dispatch goes through `self`). A generic method has one copy per type argument, which a vtable can't hold, so it needs `where Self: Sized` for the trait to stay dyn-compatible.

Syntax: `fn count_where<F: Fn(f64) -> bool>(&self, pred: F) -> usize where Self: Sized { ... }` (the `where` goes after the return type, before the body).""", "O(n) per call", "O(1)"),
    "How else could `count_where` stay callable on `dyn Metric`? What does each option cost?",
    ["Defaults call required methods through `self`, so overrides are seen.", "Generic methods need `where Self: Sized` in a dyn-compatible trait."],
    related=("L5",),
    wrong=dict(
        report_recomputes_mean=METRIC_HEAD + METRIC_TRAIT.replace(
            "        match self.mean() {\n            Some(m) => format!(\"{}: n={} mean={:.2}\", self.name(), self.values().len(), m),",
            "        let v = self.values();\n        match (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64) {\n            Some(m) => format!(\"{}: n={} mean={:.2}\", self.name(), v.len(), m),") + METRIC_TAIL,
        no_override=METRIC_HEAD + METRIC_TRAIT.split("\n    fn mean(&self) -> Option<f64> {\n        let busy")[0] + "\n}\n" + METRIC_TAIL,
    ),
))

MONEY_DISPLAY = r"""
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let abs = self.cents.unsigned_abs();
        let digits = (abs / 100).to_string();
        let mut grouped = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        let sign = if self.cents < 0 { "-" } else { "" };
        f.pad(&format!("{sign}${grouped}.{:02}", abs % 100))
    }
}
"""

CRED_DEBUG = r"""
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Credentials").field("user", &self.user).field("password", &"***").finish()
    }
}
"""


def money_types(cred_derive):
    return r"""
use std::fmt;

/// An amount of money in cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    pub cents: i64,
}

/// Login details. They get logged with {:?}.
#[derive(""" + cred_derive + r""")]
pub struct Credentials {
    pub user: String,
    pub password: String,
}
"""


MONEY_SOLUTION = money_types("Clone, PartialEq, Eq") + MONEY_DISPLAY + CRED_DEBUG

P.append(fix(
    "impl-display", "impl Display, and a Debug that hides secrets", "easy", "define-implement", ["Display", "Debug", "derive vs manual impls", "Formatter"],
    """
        Two problems:

        - `Money` has no `Display`. Print dollars with a comma every three digits and two decimals: `123456` →
          `$1,234.56`, `-50` → `-$0.50`. Any `i64` is allowed. Width and alignment must work: `{:>10}`.
        - The derived `Debug` on `Credentials` writes the password into the logs. Write `Debug` by hand so the
          password shows as `"***"` and everything else, including `{:#?}`, looks exactly as derived.

        Keep the other derives.
    """,
    money_types("Debug, Clone, PartialEq, Eq") + "\n// TODO: Display for Money.\n",
    MONEY_SOLUTION,
    [T("thousands", "Money { cents: 123456 }", "Money { cents: 123456 }.to_string()", '"$1,234.56"'),
     T("negative_under_a_dollar", "Money { cents: -50 }", "Money { cents: -50 }.to_string()", '"-$0.50"'),
     T("width", "format!(\"[{:>10}]\", Money { cents: 5 })", 'format!("[{:>10}]", Money { cents: 5 })', '"[     $0.05]"'),
     T("password_hidden", "format!(\"{:?}\", Credentials { user: \"bob\", password: \"hunter2\" })", 'format!("{:?}", c)', r'r#"Credentials { user: "bob", password: "***" }"#',
       setup='let c = Credentials { user: "bob".into(), password: "hunter2".into() };'),
     T("pretty_debug", "format!(\"{:#?}\", Credentials { user: \"bob\", .. })", 'format!("{:#?}", c)', r'"Credentials {\n    user: \"bob\",\n    password: \"***\",\n}"',
       setup='let c = Credentials { user: "bob".into(), password: "hunter2".into() };')],
    [T("zero", "Money { cents: 0 }", "Money { cents: 0 }.to_string()", '"$0.00"'),
     T("no_comma_below_1000", "Money { cents: 99999 }", "Money { cents: 99999 }.to_string()", '"$999.99"'),
     T("exactly_1000", "Money { cents: 100000 }", "Money { cents: 100000 }.to_string()", '"$1,000.00"'),
     T("i64_min", "Money { cents: i64::MIN }", "Money { cents: i64::MIN }.to_string()", '"-$92,233,720,368,547,758.08"'),
     T("i64_max", "Money { cents: i64::MAX }", "Money { cents: i64::MAX }.to_string()", '"$92,233,720,368,547,758.07"'),
     T("centered_fill", "format!(\"{:*^11}\", Money { cents: -123 })", 'format!("{:*^11}", Money { cents: -123 })', '"**-$1.23***"'),
     T("money_debug_still_derived", "format!(\"{:?}\", Money { cents: 5 })", 'format!("{:?}", Money { cents: 5 })', '"Money { cents: 5 }"'),
     T("user_is_escaped", "user with a quote: bo\"b", 'format!("{:?}", c)', r'r#"Credentials { user: "bo\"b", password: "***" }"#',
       setup='let c = Credentials { user: "bo\\"b".into(), password: "x".into() };'),
     T("empty_password_still_hidden", "password \"\"", 'format!("{:?}", c)', r'r#"Credentials { user: "", password: "***" }"#',
       setup='let c = Credentials { user: String::new(), password: String::new() };'),
     T("other_derives_kept", "c.clone() == c", "c.clone() == c", "true",
       setup='let c = Credentials { user: "a".into(), password: "b".into() };'),
     T("nested_pretty", "format!(\"{:#?}\", vec![Credentials])", 'format!("{:#?}", vec![c])', r'"[\n    Credentials {\n        user: \"é\",\n        password: \"***\",\n    },\n]"',
       setup='let c = Credentials { user: "é".into(), password: "p".into() };'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4403);
         for _ in 0..400 {
             let digits = rng.below(13) as u32;
             let cents = rng.int(-(10i64.pow(digits)), 10i64.pow(digits));
             let abs = cents.unsigned_abs();
             let mut parts = Vec::new();
             let mut d = abs / 100;
             loop {
                 parts.push(d % 1000);
                 d /= 1000;
                 if d == 0 {
                     break;
                 }
             }
             let mut body = parts.pop().unwrap().to_string();
             while let Some(p) = parts.pop() {
                 body.push_str(&format!(",{p:03}"));
             }
             let want = format!("{}${}.{:02}", if cents < 0 { "-" } else { "" }, body, abs % 100);
             check!(format!("cents = {cents}"), Money { cents }.to_string(), want);
         }
     }
     """],
    [("rust", "`write!(f, ...)` ignores the caller's width; build the text, then `f.pad(&text)`. `i64::MIN.abs()` overflows, `unsigned_abs()` doesn't, and `-50 / 100` is 0, so keep the sign apart."),
     ("rust", "Remove `Debug` from the derive list, then use `f.debug_struct(\"Credentials\").field(..).finish()`. It handles `{:#?}` and escaping; a hand-written `write!` doesn't.")],
    ("""`#[derive(Debug)]` prints every field, secrets included. A manual impl with the `debug_*` builders keeps the derived look, pretty-printing and escaping. For `Display`, `Formatter::pad` applies width, fill and alignment.

Syntax: `impl fmt::Display for Money { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { ... } }` · `f.debug_struct("Name").field("a", &self.a).finish()` · `finish_non_exhaustive()` prints `..` for hidden fields.""", "O(digits)", "O(digits)"),
    "Which derives are safe to keep on a type holding a secret, and which would you also write by hand?",
    ["Derive when the generated impl is right; write it by hand when it isn't (secrets, ignored fields).", "`f.pad` for width; `debug_struct` for Debug."],
    related=("S2", "S8"),
    wrong=dict(
        write_ignores_width=MONEY_SOLUTION.replace('f.pad(&format!("{sign}${grouped}.{:02}", abs % 100))', 'write!(f, "{sign}${grouped}.{:02}", abs % 100)'),
        debug_by_hand_with_write=money_types("Clone, PartialEq, Eq") + MONEY_DISPLAY + r"""
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Credentials {{ user: {:?}, password: \"***\" }}", self.user)
    }
}
""",
        sign_from_division=MONEY_SOLUTION.replace(
            "        let abs = self.cents.unsigned_abs();\n        let digits = (abs / 100).to_string();",
            "        let dollars = self.cents / 100;\n        let abs = (self.cents % 100).unsigned_abs();\n        let digits = dollars.unsigned_abs().to_string();").replace(
            'let sign = if self.cents < 0 { "-" } else { "" };', 'let sign = if dollars < 0 { "-" } else { "" };').replace("abs % 100))", "abs))"),
    ),
))

ASSOC_HEAD = r"""
use std::fmt::Debug;

pub struct Celsius(pub f64);
pub struct Fahrenheit(pub f64);
pub struct Kelvin(pub f64);

/// Decodes "1, 2, 3" into integers.
pub struct CsvInts;
/// Decodes "key=value" into a pair.
pub struct KeyValue;
"""

ASSOC_TRAITS = r"""
pub trait Convert<T> {
    fn convert(&self) -> T;
}

impl Convert<Fahrenheit> for Celsius {
    fn convert(&self) -> Fahrenheit {
        Fahrenheit(self.0 * 9.0 / 5.0 + 32.0)
    }
}

impl Convert<Kelvin> for Celsius {
    fn convert(&self) -> Kelvin {
        Kelvin(self.0 + 273.15)
    }
}

pub trait Decoder {
    type Output;
    const NAME: &'static str;

    fn decode(&self, input: &str) -> Option<Self::Output>;
}

impl Decoder for CsvInts {
    type Output = Vec<i64>;
    const NAME: &'static str = "csv";

    fn decode(&self, input: &str) -> Option<Vec<i64>> {
        if input.trim().is_empty() {
            return Some(Vec::new());
        }
        input.split(',').map(|p| p.trim().parse().ok()).collect()
    }
}

impl Decoder for KeyValue {
    type Output = (String, String);
    const NAME: &'static str = "kv";

    fn decode(&self, input: &str) -> Option<(String, String)> {
        let (k, v) = input.split_once('=')?;
        Some((k.trim().to_string(), v.trim().to_string()))
    }
}
"""

ASSOC_TAIL = r"""
/// Decodes every item, skipping the ones that fail.
pub fn decode_all<D: Decoder>(d: &D, items: &[&str]) -> Vec<D::Output> {
    items.iter().filter_map(|s| d.decode(s)).collect()
}

/// "<NAME>: <first item that decodes, in Debug form>", or "<NAME>: none".
pub fn first_decoded<D>(d: &D, items: &[&str]) -> String
WHERE
{
    FIRST
}

/// The temperature in the two other scales.
pub fn to_both(c: &Celsius) -> (Fahrenheit, Kelvin) {
    BOTH
}
"""

ASSOC_FILLED = ASSOC_TAIL.replace("WHERE", "where\n    D: Decoder,\n    D::Output: Debug,").replace(
    "FIRST", 'match items.iter().find_map(|s| d.decode(s)) {\n        Some(out) => format!("{}: {:?}", D::NAME, out),\n        None => format!("{}: none", D::NAME),\n    }').replace(
    "BOTH", "(c.convert(), <Celsius as Convert<Kelvin>>::convert(c))")

P.append(fix(
    "associated-types", "Associated types, consts and generic traits", "easy", "define-implement", ["associated types", "associated consts", "generic traits", "fully qualified syntax"],
    """
        Write the two traits the code at the bottom uses, then finish `first_decoded` (its `where` clause too)
        and `to_both`.

        - `Convert<T>` with `fn convert(&self) -> T`. `Celsius` converts to `Fahrenheit` (×9/5 + 32) and to
          `Kelvin` (+273.15).
        - `Decoder` with an associated type `Output`, an associated const `NAME: &'static str`, and
          `fn decode(&self, input: &str) -> Option<Self::Output>`.
          - `CsvInts` (`"csv"`) gives `Vec<i64>`. Parts are split on `,` and trimmed; a blank input is an empty
            list; any part that isn't an integer makes the whole input fail.
          - `KeyValue` (`"kv"`) gives `(String, String)`, split at the first `=`, both sides trimmed. No `=` fails.
    """,
    ASSOC_HEAD + "\n// TODO: the Convert<T> and Decoder traits, and their impls.\n" + ASSOC_TAIL.replace("WHERE", "// TODO: the where clause").replace("FIRST", "todo!()").replace("BOTH", "todo!()"),
    ASSOC_HEAD + ASSOC_TRAITS + ASSOC_FILLED,
    [T("decode_csv", "decode_all(&CsvInts, [\"1, 2\", \"x\", \"\"])", 'decode_all(&CsvInts, &["1, 2", "x", ""])', "vec![vec![1, 2], vec![]]"),
     T("decode_kv", "KeyValue.decode(\" a = b=c \")", 'KeyValue.decode(" a = b=c ")', 'Some(("a".to_string(), "b=c".to_string()))'),
     T("first_decoded_csv", "first_decoded(&CsvInts, [\"x\", \"3,4\"])", 'first_decoded(&CsvInts, &["x", "3,4"])', '"csv: [3, 4]"'),
     T("both_scales", "to_both(&Celsius(100.0))", 'format!("{:.2} {:.2}", f.0, k.0)', '"212.00 373.15"', setup="let (f, k) = to_both(&Celsius(100.0));"),
     """
     #[test]
     fn a_new_decoder() {
         struct Flag;
         impl Decoder for Flag {
             type Output = bool;
             const NAME: &'static str = "flag";
             fn decode(&self, input: &str) -> Option<bool> {
                 match input {
                     "on" => Some(true),
                     "off" => Some(false),
                     _ => None,
                 }
             }
         }
         check!("decode_all(&Flag, [\\"on\\", \\"?\\", \\"off\\"])", decode_all(&Flag, &["on", "?", "off"]), vec![true, false]);
         check!("first_decoded(&Flag, [\\"?\\"])", first_decoded(&Flag, &["?"]), "flag: none");
         check!("<Flag as Decoder>::NAME", <Flag as Decoder>::NAME, "flag");
     }
     """],
    [T("csv_bad_part_fails", "CsvInts.decode(\"1,,2\")", 'CsvInts.decode("1,,2")', "None"),
     T("csv_blank", "CsvInts.decode(\"   \")", 'CsvInts.decode("   ")', "Some(vec![])"),
     T("csv_negative", "CsvInts.decode(\"-5, 7\")", 'CsvInts.decode("-5, 7")', "Some(vec![-5, 7])"),
     T("csv_overflow_fails", "CsvInts.decode(\"99999999999999999999\")", 'CsvInts.decode("99999999999999999999")', "None"),
     T("kv_no_equals", "KeyValue.decode(\"abc\")", 'KeyValue.decode("abc")', "None"),
     T("kv_empty_sides", "KeyValue.decode(\"=\")", 'KeyValue.decode("=")', "Some((String::new(), String::new()))"),
     T("names", "CsvInts::NAME, KeyValue::NAME", "(<CsvInts as Decoder>::NAME, <KeyValue as Decoder>::NAME)", '("csv", "kv")'),
     T("first_decoded_kv", "first_decoded(&KeyValue, [\"k=v\"])", 'first_decoded(&KeyValue, &["k=v"])', r'r#"kv: ("k", "v")"#'),
     T("first_decoded_empty", "first_decoded(&CsvInts, [])", "first_decoded(&CsvInts, &[])", '"csv: none"'),
     T("convert_by_annotation", "let f: Fahrenheit = Celsius(-40.0).convert()", 'format!("{:.2}", f.0)', '"-40.00"', setup="let f: Fahrenheit = Celsius(-40.0).convert();"),
     T("absolute_zero", "<Celsius as Convert<Kelvin>>::convert(&Celsius(-273.15))", 'format!("{:.2}", <Celsius as Convert<Kelvin>>::convert(&Celsius(-273.15)).0)', '"0.00"'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4404);
         for _ in 0..300 {
             let n = rng.below(4);
             let parts: Vec<String> = (0..n).map(|_| if rng.below(8) == 0 { "x".to_string() } else { rng.int(-99, 99).to_string() }).collect();
             let input = parts.join(",");
             let want: Option<Vec<i64>> = if input.is_empty() { Some(vec![]) } else { parts.iter().map(|p| p.parse().ok()).collect() };
             check!(format!("CsvInts.decode({input:?})"), CsvInts.decode(&input), want);
             let c = rng.int(-500, 500) as f64 / 2.0;
             let (f, k) = to_both(&Celsius(c));
             check!(format!("to_both(&Celsius({c}))"), format!("{:.3} {:.3}", f.0, k.0), format!("{:.3} {:.3}", c * 1.8 + 32.0, c + 273.15));
         }
     }
     """],
    [("rust", "Declare `type Output;` and `const NAME: &'static str;` in the trait; each impl writes `type Output = Vec<i64>;` and `const NAME: &'static str = \"csv\";`."),
     ("rust", "`first_decoded` needs `where D: Decoder, D::Output: Debug`. In `to_both`, `c.convert()` has two candidates: let the expected type choose, or write `<Celsius as Convert<Kelvin>>::convert(c)`."),
     ("edge case", "Collecting an iterator of `Option<i64>` into `Option<Vec<i64>>` stops at the first `None`.")],
    ("""Use an associated type when each implementor has exactly one natural output (`Decoder::Output`); use a generic trait `Convert<T>` when one type can implement it several times. With several impls, the caller picks one by the expected type or with fully qualified syntax.

Syntax: `trait Decoder { type Output; const NAME: &'static str; fn decode(&self, input: &str) -> Option<Self::Output>; }` · bounds on an associated type: `where D: Decoder, D::Output: Debug` · `<Celsius as Convert<Kelvin>>::convert(c)`.""", "O(n) per decode", "O(n)"),
    "`Iterator` uses an associated `Item`; `From<T>` is generic. Why did std choose differently for each?",
    ["Associated types: one output per implementing type.", "Generic traits: many impls per type, picked by type annotation or fully qualified syntax.", "Bounds can constrain associated types: `D::Output: Debug`."],
    related=("L5",),
    wrong=dict(
        kv_splits_at_last=ASSOC_HEAD + ASSOC_TRAITS.replace("input.split_once('=')?", "input.rsplit_once('=')?") + ASSOC_FILLED,
        csv_skips_bad_parts=ASSOC_HEAD + ASSOC_TRAITS.replace("input.split(',').map(|p| p.trim().parse().ok()).collect()", "Some(input.split(',').filter_map(|p| p.trim().parse().ok()).collect())") + ASSOC_FILLED,
    ),
))

ITER_ARGS_STARTER = r"""
use std::fmt::Display;

/// Total number of characters (not bytes) across the words.
pub fn total_chars(words: &[String]) -> usize {
    words.iter().map(|w| w.chars().count()).sum()
}

/// The items formatted with Display, joined with `sep`.
pub fn join_display(items: &[i32], sep: &str) -> String {
    items.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(sep)
}

/// How many words `keep` accepts.
pub fn count_where(words: &[String], keep: fn(&str) -> bool) -> usize {
    words.iter().filter(|w| keep(w)).count()
}

/// `f`, then `g`.
pub fn compose(f: fn(i64) -> i64, g: fn(i64) -> i64) -> Box<dyn Fn(i64) -> i64> {
    Box::new(move |x| g(f(x)))
}

/// The even numbers from 0 up to `limit`, inclusive.
pub fn evens(limit: u64) -> std::vec::IntoIter<u64> {
    (0..=limit).filter(|n| n % 2 == 0).collect::<Vec<_>>().into_iter()
}

/// The items in order, or in reverse when `descending`.
pub fn ordered(v: &[i32], descending: bool) -> impl Iterator<Item = &i32> {
    if descending {
        v.iter().rev()
    } else {
        v.iter()
    }
}
"""

ITER_ARGS_SOLUTION = r"""
use std::fmt::Display;

/// Total number of characters (not bytes) across the words.
pub fn total_chars(words: impl IntoIterator<Item = impl AsRef<str>>) -> usize {
    words.into_iter().map(|w| w.as_ref().chars().count()).sum()
}

/// The items formatted with Display, joined with `sep`.
pub fn join_display(items: impl IntoIterator<Item = impl Display>, sep: &str) -> String {
    items.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(sep)
}

/// How many words `keep` accepts.
pub fn count_where(words: impl IntoIterator<Item = impl AsRef<str>>, keep: impl Fn(&str) -> bool) -> usize {
    words.into_iter().filter(|w| keep(w.as_ref())).count()
}

/// `f`, then `g`.
pub fn compose(f: impl Fn(i64) -> i64, g: impl Fn(i64) -> i64) -> impl Fn(i64) -> i64 {
    move |x| g(f(x))
}

/// The even numbers from 0 up to `limit`, inclusive.
pub fn evens(limit: u64) -> impl Iterator<Item = u64> + Clone {
    (0..=limit).step_by(2)
}

/// The items in order, or in reverse when `descending`.
pub fn ordered(v: &[i32], descending: bool) -> Box<dyn Iterator<Item = &i32> + '_> {
    if descending {
        Box::new(v.iter().rev())
    } else {
        Box::new(v.iter())
    }
}
"""

P.append(fix(
    "impl-trait-arguments", "impl Trait in argument and return position", "easy", "define-implement", ["impl Trait", "IntoIterator", "AsRef", "Box<dyn Iterator>"],
    """
        These functions take and return types that are too specific. The tests call them with arrays of `&str`,
        `Vec`s, ranges, `split` iterators and capturing closures, clone what `evens` returns, and call
        `evens(u64::MAX)`, so it must be lazy. `ordered` doesn't compile at all.

        Change the signatures (and bodies where needed) so every call works. Keep what each function computes.
    """,
    ITER_ARGS_STARTER,
    ITER_ARGS_SOLUTION,
    [T("owned_vec_of_strings", "vec![\"héllo\", \"ab\"] as Strings", 'total_chars(vec!["héllo".to_string(), "ab".to_string()])', "7"),
     T("join_a_range", "1..=3, \", \"", 'join_display(1..=3, ", ")', '"1, 2, 3"'),
     T("closure_captures", "words longer than min = 3", "count_where(&words, |w| w.len() > min)", "2",
       setup='let min = 3;\nlet words = vec!["tree".to_string(), "sky".to_string(), "forest".to_string()];'),
     T("compose_closures", "compose(x + k, x * 2)(1) with k = 3", "h(1)", "8", setup="let k = 3;\nlet h = compose(move |x| x + k, |x| x * 2);"),
     T("evens_is_lazy_and_clone", "evens(u64::MAX).take(3), cloned", "(e.clone().collect::<Vec<_>>(), e.count())", "(vec![0, 2, 4], 3)",
       setup="let e = evens(u64::MAX).take(3);"),
     T("ordered_both_ways", "ordered(&[1, 2, 3], true / false)", "(ordered(&v, true).copied().collect::<Vec<_>>(), ordered(&v, false).copied().collect::<Vec<_>>())", "(vec![3, 2, 1], vec![1, 2, 3])",
       setup="let v = [1, 2, 3];")],
    [T("array_of_str", "[\"a\", \"bc\"]", 'total_chars(["a", "bc"])', "3"),
     T("split_iterator", "\"one two\".split(' ')", "total_chars(\"one two\".split(' '))", "6"),
     T("empty_iter", "std::iter::empty::<&str>()", "total_chars(std::iter::empty::<&str>())", "0"),
     T("borrowed_vec_still_usable", "&v, then v.len()", "(total_chars(&v), v.len())", "(5, 2)",
       setup='let v = vec!["abc".to_string(), "de".to_string()];'),
     T("join_floats_by_ref", "&[1.5, 2.0], \"|\"", 'join_display(&[1.5, 2.0], "|")', '"1.5|2"'),
     T("join_empty", "Vec::<i32>::new()", 'join_display(Vec::<i32>::new(), ",")', "String::new()"),
     T("fn_item_still_works", "[\"\", \"a\"], str::is_empty", 'count_where(["", "a"], str::is_empty)', "1"),
     T("compose_order", "compose(x * 10, x + 1)(2)", "compose(|x| x * 10, |x| x + 1)(2)", "21"),
     T("compose_nested", "compose(compose(+1, *2), -3)(5)", "compose(compose(|x| x + 1, |x| x * 2), |x| x - 3)(5)", "9"),
     T("evens_inclusive", "evens(6)", "evens(6).collect::<Vec<_>>()", "vec![0, 2, 4, 6]"),
     T("evens_zero", "evens(0)", "evens(0).collect::<Vec<_>>()", "vec![0]"),
     T("ordered_empty", "ordered(&[], true)", "ordered(&[], true).count()", "0"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4405);
         for _ in 0..300 {
             let n = rng.below(5);
             let words: Vec<String> = (0..n).map(|_| { let len = rng.below(4); rng.string(len, "aé") }).collect();
             let chars: usize = words.iter().map(|w| w.chars().count()).sum();
             check!(format!("total_chars({words:?})"), total_chars(&words), chars);
             check!(format!("join_display({words:?}, \"/\")"), join_display(&words, "/"), words.join("/"));
             let limit = rng.below(20) as u64;
             check!(format!("evens({limit})"), evens(limit).collect::<Vec<_>>(), (0..=limit).filter(|x| x % 2 == 0).collect::<Vec<_>>());
             let v: Vec<i32> = rng.vec(n, -9, 9);
             let desc = rng.bool();
             let mut want = v.clone();
             if desc {
                 want.reverse();
             }
             check!(format!("ordered({v:?}, {desc})"), ordered(&v, desc).copied().collect::<Vec<_>>(), want);
         }
     }
     """],
    [("rust", "Arguments: `impl IntoIterator<Item = impl AsRef<str>>`, `impl Display` items, `impl Fn(&str) -> bool` (a `fn` pointer can't capture). Returns: `impl Fn(i64) -> i64` and `impl Iterator<Item = u64> + Clone`: callers may only use what the signature promises."),
     ("rust", "`impl Trait` in return position is one concrete type, so `if`/`else` returning `Rev<Iter>` and `Iter` fails (E0308). Box both branches as `Box<dyn Iterator<Item = &i32> + '_>`.")],
    ("""In argument position `impl Trait` is an anonymous generic: the caller picks the type and each call site is monomorphised. In return position it's one hidden type the function picks, and callers see only the listed traits (so `+ Clone` must be written to be usable). Two different types need `Box<dyn Trait + '_>` (or an enum like `Either`).

Syntax: `fn f(xs: impl IntoIterator<Item = impl AsRef<str>>)` · `fn g() -> impl Iterator<Item = u64> + Clone` · `-> Box<dyn Iterator<Item = &i32> + '_>` (`'_` ties the box to the borrowed input).""", "O(n)", "O(1) extra (evens is lazy)"),
    "Why can't a caller write `compose::<SomeType, _>(...)` with `impl Trait` arguments, and when does that matter?",
    ["`impl Trait` arguments: caller-chosen generic.", "`impl Trait` returns: one callee-chosen type; list every trait callers need.", "Different types per branch → `Box<dyn Trait + '_>`."],
    related=("L5", "S6"),
    wrong=dict(
        bytes_not_chars=ITER_ARGS_SOLUTION.replace("w.as_ref().chars().count()", "w.as_ref().len()"),
        compose_backwards=ITER_ARGS_SOLUTION.replace("move |x| g(f(x))", "move |x| f(g(x))"),
        evens_exclusive=ITER_ARGS_SOLUTION.replace("(0..=limit).step_by(2)", "(0..limit).step_by(2)"),
    ),
))

# ---------------------------------------------------------------- static vs dynamic (medium)

BOTH_WAYS_HEAD = r"""
use std::f64::consts::PI;

pub trait Shape {
    fn area(&self) -> f64;
    fn name(&self) -> String;
}

pub struct Circle {
    pub radius: f64,
}

pub struct Rectangle {
    pub width: f64,
    pub height: f64,
}

impl Shape for Circle {
    fn area(&self) -> f64 {
        PI * self.radius * self.radius
    }

    fn name(&self) -> String {
        format!("circle({})", self.radius)
    }
}

impl Shape for Rectangle {
    fn area(&self) -> f64 {
        self.width * self.height
    }

    fn name(&self) -> String {
        format!("rect({}x{})", self.width, self.height)
    }
}
"""

BOTH_WAYS_FNS = r"""
/// Static dispatch: compiled once per `T`, calls resolved at compile time.
pub fn total_area_generic<T: Shape>(shapes: &[T]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

/// Dynamic dispatch: compiled once, every call goes through the vtable.
pub fn total_area_dyn(shapes: &[&dyn Shape]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}

/// The shape with the largest area; the first of them on a tie.
pub fn largest<T: Shape>(shapes: &[T]) -> Option<&T> {
    LARGEST
}
"""

BOTH_WAYS_FORWARD = r"""
impl<S: Shape + ?Sized> Shape for Box<S> {
    fn area(&self) -> f64 {
        (**self).area()
    }

    fn name(&self) -> String {
        (**self).name()
    }
}

impl<S: Shape + ?Sized> Shape for &S {
    fn area(&self) -> f64 {
        (**self).area()
    }

    fn name(&self) -> String {
        (**self).name()
    }
}
"""

LARGEST_BUGGY = "shapes.iter().max_by(|a, b| a.area().total_cmp(&b.area()))"
LARGEST_FIXED = "shapes.iter().reduce(|best, s| if s.area() > best.area() { s } else { best })"

BOTH_WAYS_SOLUTION = BOTH_WAYS_HEAD + BOTH_WAYS_FORWARD + BOTH_WAYS_FNS.replace("LARGEST", LARGEST_FIXED)

P.append(fix(
    "shapes-both-ways", "Shapes both ways: generic vs dyn", "medium", "static-vs-dynamic", ["static dispatch", "dyn Trait", "?Sized", "forwarding impls"],
    """
        Two bugs:

        - The tests pass `Vec<Box<dyn Shape>>`, `Vec<&dyn Shape>` and `Vec<&Circle>` to the generic functions.
          None of that compiles (E0277).
        - `largest` returns the last of several equally large shapes, not the first.

        Fix both without changing any function's signature.
    """,
    BOTH_WAYS_HEAD + BOTH_WAYS_FNS.replace("LARGEST", LARGEST_BUGGY),
    BOTH_WAYS_SOLUTION,
    [T("generic_on_one_type", "total_area_generic(&[Circle 1, Circle 2])", 'format!("{:.4}", total_area_generic(&[Circle { radius: 1.0 }, Circle { radius: 2.0 }]))', '"15.7080"'),
     T("generic_on_boxes", "total_area_generic(&[Box Circle 1, Box Rect 2x3])", 'format!("{:.4}", total_area_generic(&boxes))', '"9.1416"',
       setup="let boxes: Vec<Box<dyn Shape>> = vec![Box::new(Circle { radius: 1.0 }), Box::new(Rectangle { width: 2.0, height: 3.0 })];"),
     T("dyn_on_refs", "total_area_dyn(&[&Rect 1x1, &Rect 2x2])", "total_area_dyn(&[&a, &b])", "5.0",
       setup="let a = Rectangle { width: 1.0, height: 1.0 };\nlet b = Rectangle { width: 2.0, height: 2.0 };"),
     T("largest_tie_is_first", "largest(&[Rect 2x3, Rect 3x2])", "largest(&[Rectangle { width: 2.0, height: 3.0 }, Rectangle { width: 3.0, height: 2.0 }]).map(|s| s.name())", 'Some("rect(2x3)".to_string())'),
     T("largest_boxed", "largest(&[Box Circle 1, Box Rect 1x4])", "largest(&boxes).map(|s| s.name())", 'Some("rect(1x4)".to_string())',
       setup="let boxes: Vec<Box<dyn Shape>> = vec![Box::new(Circle { radius: 1.0 }), Box::new(Rectangle { width: 1.0, height: 4.0 })];"),
     T("largest_empty", "largest::<Circle>(&[])", "largest::<Circle>(&[]).is_none()", "true")],
    [T("generic_on_dyn_refs", "total_area_generic(&[&dyn Rect 1x2, &dyn Rect 3x1])", "total_area_generic(&refs)", "5.0",
       setup="let (a, b) = (Rectangle { width: 1.0, height: 2.0 }, Rectangle { width: 3.0, height: 1.0 });\nlet refs: Vec<&dyn Shape> = vec![&a, &b];"),
     T("generic_on_plain_refs", "total_area_generic(&[&Rect 2x2])", "total_area_generic(&[&r])", "4.0", setup="let r = Rectangle { width: 2.0, height: 2.0 };"),
     T("box_of_box", "total_area_generic(&[Box<Box<Rect 3x3>>])", "total_area_generic(&[Box::new(Box::new(Rectangle { width: 3.0, height: 3.0 }))])", "9.0"),
     T("largest_dyn_tie", "largest(&[&dyn Rect 1x6, &dyn Rect 6x1, &dyn Rect 2x3])", "largest(&refs).map(|s| s.name())", 'Some("rect(1x6)".to_string())',
       setup="let (a, b, c) = (Rectangle { width: 1.0, height: 6.0 }, Rectangle { width: 6.0, height: 1.0 }, Rectangle { width: 2.0, height: 3.0 });\nlet refs: Vec<&dyn Shape> = vec![&a, &b, &c];"),
     T("largest_tie_in_the_middle", "areas [1, 4, 4, 2]", "largest(&v).map(|s| s.name())", 'Some("rect(2x2)".to_string())',
       setup="let v = [Rectangle { width: 1.0, height: 1.0 }, Rectangle { width: 2.0, height: 2.0 }, Rectangle { width: 4.0, height: 1.0 }, Rectangle { width: 2.0, height: 1.0 }];"),
     T("largest_all_zero", "areas [0, 0]", "largest(&[Rectangle { width: 0.0, height: 1.0 }, Rectangle { width: 1.0, height: 0.0 }]).map(|s| s.name())", 'Some("rect(0x1)".to_string())'),
     T("largest_returns_the_element", "largest points into the slice", "std::ptr::eq(largest(&v).unwrap(), &v[1])", "true",
       setup="let v = [Circle { radius: 1.0 }, Circle { radius: 3.0 }, Circle { radius: 2.0 }];"),
     T("name_through_box", "Box<dyn Shape>::name", "b.name()", '"circle(2.5)"', setup="let b: Box<dyn Shape> = Box::new(Circle { radius: 2.5 });"),
     T("empty_totals", "total_area_generic::<Box<dyn Shape>>(&[]), total_area_dyn(&[])", "(total_area_generic::<Box<dyn Shape>>(&[]), total_area_dyn(&[]))", "(0.0, 0.0)"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4406);
         for _ in 0..300 {
             let n = rng.below(6);
             let dims: Vec<(f64, f64)> = (0..n).map(|_| (rng.int(0, 3) as f64, rng.int(0, 3) as f64)).collect();
             let boxes: Vec<Box<dyn Shape>> = dims.iter().map(|&(w, h)| Box::new(Rectangle { width: w, height: h }) as Box<dyn Shape>).collect();
             let mut best: Option<usize> = None;
             for (i, &(w, h)) in dims.iter().enumerate() {
                 if best.map_or(true, |b| w * h > dims[b].0 * dims[b].1) {
                     best = Some(i);
                 }
             }
             let want = best.map(|i| format!("rect({}x{})", dims[i].0, dims[i].1));
             check!(format!("largest({dims:?})"), largest(&boxes).map(|s| s.name()), want);
             check!(format!("total({dims:?})"), total_area_generic(&boxes), dims.iter().map(|&(w, h)| w * h).sum::<f64>());
         }
     }
     """],
    [("rust", "`Box<dyn Shape>` is not a `Shape`: only `dyn Shape` is. Forward the trait: `impl<S: Shape + ?Sized> Shape for Box<S>`, and the same for `&S`. Without `?Sized`, `S` can't be `dyn Shape`."),
     ("rust", "Inside the forwarding impl, call `(**self).area()`. Plain `self.area()` finds the Box impl again and recurses forever."),
     ("edge case", "`max_by` returns the last maximum. Use `reduce` with a strict `>` (or `min_by` on the reversed order).")],
    ("""`T: Shape` is monomorphised: a copy per concrete type, calls inlined. `&dyn Shape` is a fat pointer (data + vtable, two words), and each call is an indirect jump. Forwarding impls with `?Sized` let one generic function accept owned values, references and trait objects alike; std does the same for `Iterator`, `Read`, `Fn` and others.

Syntax: `impl<S: Shape + ?Sized> Shape for Box<S> { fn area(&self) -> f64 { (**self).area() } }`. Every generic parameter has an implicit `Sized` bound; `?Sized` removes it.""", "O(n)", "O(1)"),
    "What does `total_area_generic::<Box<dyn Shape>>` compile to: static calls, dynamic calls, or both?",
    ["Generic = monomorphised, static calls; `dyn` = one copy, vtable calls through a fat pointer.", "`impl<S: Trait + ?Sized> Trait for Box<S>` makes boxes and trait objects usable with generic code.",
     "`max_by` keeps the last maximum."],
    related=("L5", "S7"),
    wrong=dict(
        forwarding_recurses=BOTH_WAYS_HEAD + BOTH_WAYS_FORWARD.replace("(**self)", "self") + BOTH_WAYS_FNS.replace("LARGEST", LARGEST_FIXED),
        still_last_on_tie=BOTH_WAYS_HEAD + BOTH_WAYS_FORWARD + BOTH_WAYS_FNS.replace("LARGEST", LARGEST_BUGGY),
    ),
))

CLOSURES_SOLUTION = r"""
/// Applies `f` to `start` `n` times. Generic: one copy per closure type.
pub fn apply_n<F: FnMut(i64) -> i64>(start: i64, n: u32, mut f: F) -> i64 {
    let mut x = start;
    for _ in 0..n {
        x = f(x);
    }
    x
}

/// The same through a trait object: one copy, called through a vtable.
pub fn apply_n_dyn(start: i64, n: u32, f: &mut dyn FnMut(i64) -> i64) -> i64 {
    let mut x = start;
    for _ in 0..n {
        x = f(x);
    }
    x
}

/// Steps run in the order they were pushed. Steps may borrow local state (`'a`), and a whole
/// pipeline can be moved to another thread.
pub struct Pipeline<'a> {
    steps: Vec<Box<dyn FnMut(i64) -> i64 + Send + 'a>>,
}

impl<'a> Pipeline<'a> {
    pub fn new() -> Self {
        Pipeline { steps: Vec::new() }
    }

    pub fn push(&mut self, step: impl FnMut(i64) -> i64 + Send + 'a) -> &mut Self {
        self.steps.push(Box::new(step));
        self
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Threads `x` through every step.
    pub fn run(&mut self, x: i64) -> i64 {
        self.steps.iter_mut().fold(x, |acc, step| step(acc))
    }
}
"""

P.append(write(
    "closures-generic-vs-dyn", "Closures: generic vs dyn", "medium", "static-vs-dynamic", ["FnMut", "dyn FnMut", "Box<dyn Trait + Send + 'a>"],
    """
        - `apply_n` and `apply_n_dyn` apply `f` to `start`, `n` times: one generic, one through `&mut dyn FnMut`.
        - `Pipeline` stores steps of different closure types and threads a value through them in push order.
          Steps may mutate what they capture, may borrow local variables (the `'a`), and a pipeline must be
          movable to another thread. Choose the field type.
    """,
    r"""
    use std::marker::PhantomData;

    /// Applies `f` to `start` `n` times. Generic: one copy per closure type.
    pub fn apply_n<F: FnMut(i64) -> i64>(start: i64, n: u32, f: F) -> i64 {
        todo!()
    }

    /// The same through a trait object: one copy, called through a vtable.
    pub fn apply_n_dyn(start: i64, n: u32, f: &mut dyn FnMut(i64) -> i64) -> i64 {
        todo!()
    }

    /// Steps run in the order they were pushed. Steps may borrow local state (`'a`), and a whole
    /// pipeline can be moved to another thread.
    pub struct Pipeline<'a> {
        // TODO: the steps. (Remove this placeholder.)
        _todo: PhantomData<&'a ()>,
    }

    impl<'a> Pipeline<'a> {
        pub fn new() -> Self {
            todo!()
        }

        pub fn push(&mut self, step: impl FnMut(i64) -> i64 + Send + 'a) -> &mut Self {
            todo!()
        }

        pub fn len(&self) -> usize {
            todo!()
        }

        /// Threads `x` through every step.
        pub fn run(&mut self, x: i64) -> i64 {
            todo!()
        }
    }
    """,
    CLOSURES_SOLUTION,
    [T("apply_n_doubles", "apply_n(1, 5, |x| x * 2)", "apply_n(1, 5, |x| x * 2)", "32"),
     T("apply_n_dyn_counts_calls", "apply_n_dyn(0, 3, &mut counting), then calls", "(apply_n_dyn(0, 3, &mut counting), calls)", "(3, 3)",
       setup="let mut calls = 0;\nlet mut counting = |x: i64| {\n    calls += 1;\n    x + 1\n};"),
     T("pipeline_in_push_order", "push(+1), push(*2); run(5)", "p.run(5)", "12", setup="let mut p = Pipeline::new();\np.push(|x| x + 1).push(|x| x * 2);"),
     T("steps_borrow_locals", "a step pushes every input into a local Vec; run(1), run(10)", "seen", "vec![1, 10]",
       setup="let mut seen = Vec::new();\n{\n    let mut p = Pipeline::new();\n    p.push(|x| {\n        seen.push(x);\n        x\n    });\n    p.run(1);\n    p.run(10);\n}"),
     T("runs_on_another_thread", "push(*3); run(7) on a scoped thread", "std::thread::scope(|s| s.spawn(|| p.run(7)).join().unwrap())", "21",
       setup="let mut p = Pipeline::new();\np.push(|x| x * 3);")],
    [T("apply_n_zero_times", "apply_n(9, 0, |x| x + 1)", "apply_n(9, 0, |x| x + 1)", "9"),
     T("apply_n_stateful", "apply_n(0, 4, add 1, 2, 3, 4)", "apply_n(0, 4, |x| {\n        k += 1;\n        x + k\n    })", "10", setup="let mut k = 0;"),
     T("apply_n_dyn_zero_calls", "apply_n_dyn(5, 0, ..) doesn't call f", "(apply_n_dyn(5, 0, &mut f), calls)", "(5, 0)",
       setup="let mut calls = 0;\nlet mut f = |x: i64| {\n    calls += 1;\n    x\n};"),
     T("empty_pipeline", "Pipeline::new().run(4)", "(p.run(4), p.len())", "(4, 0)", setup="let mut p = Pipeline::new();"),
     T("state_kept_between_runs", "running total step; run(1), run(2), run(3)", "(p.run(1), p.run(2), p.run(3))", "(1, 3, 6)",
       setup="let mut p = Pipeline::new();\nlet mut total = 0;\np.push(move |x| {\n    total += x;\n    total\n});"),
     T("order_matters", "push(*2), push(+1); run(5)", "p.run(5)", "11", setup="let mut p = Pipeline::new();\np.push(|x| x * 2).push(|x| x + 1);"),
     T("len_counts_steps", "three pushes", "p.len()", "3", setup="let mut p = Pipeline::new();\np.push(|x| x).push(|x| x).push(|x| -x);"),
     T("moved_owned_state", "a step owning a Vec of offsets", "p.run(0)", "6", setup="let offsets = vec![1, 2, 3];\nlet mut p = Pipeline::new();\np.push(move |x| x + offsets.iter().sum::<i64>());"),
     """
     #[test]
     fn pipeline_is_send() {
         fn assert_send<T: Send>(_: &T) {}
         let mut hits = 0;
         let mut p = Pipeline::new();
         p.push(|x| {
             hits += 1;
             x - 1
         });
         assert_send(&p);
         let got = std::thread::scope(|s| s.spawn(move || p.run(10)).join().unwrap());
         check!("run(10) on another thread, then hits", (got, hits), (9, 1));
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4407);
         for _ in 0..300 {
             let n = rng.below(6);
             let ops: Vec<(bool, i64)> = (0..n).map(|_| (rng.bool(), rng.int(-3, 3))).collect();
             let start = rng.int(-10, 10);
             let mut p = Pipeline::new();
             for &(add, k) in &ops {
                 if add {
                     p.push(move |x| x + k);
                 } else {
                     p.push(move |x| x * k);
                 }
             }
             let want = ops.iter().fold(start, |x, &(add, k)| if add { x + k } else { x * k });
             check!(format!("ops = {ops:?}, start = {start}"), p.run(start), want);
             let times = rng.below(8) as u32;
             check!(format!("apply_n({start}, {times}, +3)"), apply_n(start, times, |x| x + 3), start + 3 * times as i64);
         }
     }
     """],
    [("rust", "Different closures have different types, so the Vec holds `Box<dyn FnMut(i64) -> i64 + ...>`. A bare `Box<dyn FnMut(..)>` means `+ 'static`, which rejects closures that borrow locals; and the box must say `+ Send` for the pipeline to be `Send`."),
     ("rust", "Calling an `FnMut` needs `&mut`: `mut f: F` in the parameter list, and `iter_mut()` over the boxes.")],
    ("""Generic `F: FnMut` gets its own compiled copy per closure and can inline it; `&mut dyn FnMut` is one copy with an indirect call. To store closures of different types you need trait objects, and the object type carries every promise: auto traits (`+ Send`) and the borrow region (`+ 'a`). Without `+ 'a`, `Box<dyn Trait>` defaults to `'static`.

Syntax: `Vec<Box<dyn FnMut(i64) -> i64 + Send + 'a>>` · `fn push(&mut self, step: impl FnMut(i64) -> i64 + Send + 'a)`.""", "O(n) per run", "O(steps)"),
    "The steps could be `Box<dyn Fn>` instead of `FnMut`. What would callers lose, and what would `Pipeline` gain?",
    ["`Box<dyn Trait>` means `Box<dyn Trait + 'static>`; write `+ 'a` to allow borrows.", "Auto traits must be named on the object type: `dyn FnMut(..) + Send`.", "FnMut needs `&mut` to call."],
    related=("L6", "C1"),
    wrong=dict(
        run_reversed=sub(CLOSURES_SOLUTION, "self.steps.iter_mut().fold", "self.steps.iter_mut().rev().fold"),
        apply_n_one_extra=sub(CLOSURES_SOLUTION, "    let mut x = start;\n    for _ in 0..n {\n        x = f(x);\n    }\n    x\n}\n\n/// The same", "    let mut x = start;\n    for _ in 0..=n {\n        x = f(x);\n    }\n    x\n}\n\n/// The same"),
    ),
))

BUS_STARTER = r"""
use std::cell::Cell;
use std::rc::Rc;
use std::thread;

pub trait Handler {
    /// A reply to `event`, or None.
    fn handle(&self, event: &str) -> Option<String>;
}

/// Counts the events it sees. Whoever holds a clone of `count` can read it.
pub struct Counter {
    pub count: Rc<Cell<usize>>,
}

impl Handler for Counter {
    fn handle(&self, _event: &str) -> Option<String> {
        self.count.set(self.count.get() + 1);
        None
    }
}

/// Replies with `prefix` + the event.
pub struct Echo {
    pub prefix: String,
}

impl Handler for Echo {
    fn handle(&self, event: &str) -> Option<String> {
        Some(format!("{}{}", self.prefix, event))
    }
}

#[derive(Default)]
pub struct Bus {
    handlers: Vec<Box<dyn Handler>>,
}

impl Bus {
    pub fn register(&mut self, h: Box<dyn Handler>) {
        self.handlers.push(h);
    }

    /// For each event in order, every handler's reply in registration order.
    pub fn dispatch(&self, events: &[&str]) -> Vec<String> {
        let mut out = Vec::new();
        for e in events {
            for h in &self.handlers {
                out.extend(h.handle(e));
            }
        }
        out
    }
}

/// Dispatches on a worker thread.
pub fn dispatch_in_background(bus: Bus, events: Vec<String>) -> Vec<String> {
    thread::spawn(move || {
        let refs: Vec<&str> = events.iter().map(String::as_str).collect();
        bus.dispatch(&refs)
    })
    .join()
    .unwrap()
}
"""

BUS_SOLUTION = (BUS_STARTER
                .replace("use std::cell::Cell;\nuse std::rc::Rc;\n", "use std::sync::atomic::{AtomicUsize, Ordering};\nuse std::sync::Arc;\n")
                .replace("pub count: Rc<Cell<usize>>,", "pub count: Arc<AtomicUsize>,")
                .replace("self.count.set(self.count.get() + 1);", "self.count.fetch_add(1, Ordering::Relaxed);")
                .replace("Vec<Box<dyn Handler>>", "Vec<Box<dyn Handler + Send>>")
                .replace("h: Box<dyn Handler>", "h: Box<dyn Handler + Send>"))

P.append(fix(
    "fix-dyn-send", "Fix: dyn Handler can't be sent between threads (E0277)", "medium", "static-vs-dynamic", ["E0277", "Send", "auto traits", "dyn Trait + Send"],
    """
        `dispatch_in_background` doesn't compile: `dyn Handler` cannot be sent between threads safely. Make a `Bus`
        sendable. `Counter` must still share its count with the caller, now as `Arc<AtomicUsize>`.

        Handlers that are `Send` but not `Sync` (a `Cell` inside, say) must still be accepted.
    """,
    BUS_STARTER,
    BUS_SOLUTION,
    [T("echo_in_background", "Echo \"> \", events [a, b]", 'dispatch_in_background(bus, vec!["a".into(), "b".into()])', 'vec!["> a", "> b"]',
       setup='let mut bus = Bus::default();\nbus.register(Box::new(Echo { prefix: "> ".into() }));'),
     T("counter_shared_with_caller", "Counter, three events in background, then read count", "count.load(std::sync::atomic::Ordering::SeqCst)", "3",
       setup='let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\nlet mut bus = Bus::default();\nbus.register(Box::new(Counter { count: count.clone() }));\ndispatch_in_background(bus, vec!["x".into(), "y".into(), "z".into()]);'),
     T("dispatch_order", "Echo 1, Echo 2; events [a, b]", 'bus.dispatch(&["a", "b"])', 'vec!["1a", "2a", "1b", "2b"]',
       setup='let mut bus = Bus::default();\nbus.register(Box::new(Echo { prefix: "1".into() }));\nbus.register(Box::new(Echo { prefix: "2".into() }));'),
     T("no_handlers", "empty bus", 'dispatch_in_background(Bus::default(), vec!["a".into()])', "Vec::<String>::new()"),
     """
     #[test]
     fn send_but_not_sync_handler() {
         // Cell is Send but not Sync: a bus that moves to one thread doesn't need Sync.
         struct Tally(std::cell::Cell<u32>);
         impl Handler for Tally {
             fn handle(&self, e: &str) -> Option<String> {
                 self.0.set(self.0.get() + 1);
                 Some(format!("{e}#{}", self.0.get()))
             }
         }
         let mut bus = Bus::default();
         bus.register(Box::new(Tally(std::cell::Cell::new(0))));
         check!("Tally, events [a, b]", dispatch_in_background(bus, vec!["a".into(), "b".into()]), vec!["a#1", "b#2"]);
     }
     """],
    [T("no_events", "Echo, no events", "dispatch_in_background(bus, vec![])", "Vec::<String>::new()",
       setup='let mut bus = Bus::default();\nbus.register(Box::new(Echo { prefix: "!".into() }));'),
     T("counter_skipped_in_replies", "Counter then Echo; events [e]", 'bus.dispatch(&["e"])', 'vec!["~e"]',
       setup='let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\nlet mut bus = Bus::default();\nbus.register(Box::new(Counter { count }));\nbus.register(Box::new(Echo { prefix: "~".into() }));'),
     T("counter_zero_events", "Counter, no events", "count.load(std::sync::atomic::Ordering::SeqCst)", "0",
       setup='let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\nlet mut bus = Bus::default();\nbus.register(Box::new(Counter { count: count.clone() }));\ndispatch_in_background(bus, vec![]);'),
     T("two_counters_one_count", "two Counters sharing one count, two events", "count.load(std::sync::atomic::Ordering::SeqCst)", "4",
       setup='let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));\nlet mut bus = Bus::default();\nbus.register(Box::new(Counter { count: count.clone() }));\nbus.register(Box::new(Counter { count: count.clone() }));\ndispatch_in_background(bus, vec!["a".into(), "b".into()]);'),
     T("unicode_events", "Echo \"» \", events [é, 日本]", 'dispatch_in_background(bus, vec!["é".into(), "日本".into()])', 'vec!["» é", "» 日本"]',
       setup='let mut bus = Bus::default();\nbus.register(Box::new(Echo { prefix: "» ".into() }));'),
     T("dispatch_on_this_thread_too", "Echo, dispatch(&[q])", 'bus.dispatch(&["q"])', 'vec!["-q"]',
       setup='let mut bus = Bus::default();\nbus.register(Box::new(Echo { prefix: "-".into() }));'),
     """
     #[test]
     fn bus_is_send() {
         fn assert_send<T: Send>(_: &T) {}
         let bus = Bus::default();
         assert_send(&bus);
         check!("Bus is Send", true, true);
     }
     """,
     """
     #[test]
     fn handler_with_non_static_free_data() {
         // A handler owning a Vec, moved to the worker with the bus.
         struct Replies(Vec<&'static str>);
         impl Handler for Replies {
             fn handle(&self, e: &str) -> Option<String> {
                 self.0.iter().find(|r| r.starts_with(e)).map(|r| r.to_string())
             }
         }
         let mut bus = Bus::default();
         bus.register(Box::new(Replies(vec!["hi there", "bye now"])));
         check!("Replies, events [bye, hi, x]", dispatch_in_background(bus, vec!["bye".into(), "hi".into(), "x".into()]), vec!["bye now", "hi there"]);
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4408);
         for _ in 0..200 {
             let hn = rng.below(4);
             let prefixes: Vec<String> = (0..hn).map(|_| { let len = rng.below(3); rng.string(len, "pq") }).collect();
             let en = rng.below(4);
             let events: Vec<String> = (0..en).map(|_| { let len = rng.below(3); rng.string(len, "ab") }).collect();
             let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
             let mut bus = Bus::default();
             bus.register(Box::new(Counter { count: count.clone() }));
             for p in &prefixes {
                 bus.register(Box::new(Echo { prefix: p.clone() }));
             }
             let mut want = Vec::new();
             for e in &events {
                 for p in &prefixes {
                     want.push(format!("{p}{e}"));
                 }
             }
             check!(format!("prefixes = {prefixes:?}, events = {events:?}"), dispatch_in_background(bus, events.clone()), want);
             check!(format!("count after {events:?}"), count.load(std::sync::atomic::Ordering::SeqCst), events.len());
         }
     }
     """],
    [("rust", "A trait object only has the auto traits its type names. `Box<dyn Handler>` isn't `Send` even if every handler happens to be; write `Box<dyn Handler + Send>` (or make `Send` a supertrait)."),
     ("rust", "Then `Counter` stops qualifying: `Rc` and `Cell` aren't `Send`. `Arc<AtomicUsize>` is the thread-safe version of `Rc<Cell<usize>>`."),
     ("edge case", "Moving the bus to one thread needs `Send`, not `Sync`. Asking for `+ Send + Sync` would reject handlers that hold a `Cell`.")],
    ("""`Send` and `Sync` are auto traits: the compiler implements them for a concrete type from its fields, but a trait object erases the type, so `dyn Handler` has only the auto traits you write (`dyn Handler + Send`). Putting `Send` in the object type (or as a supertrait) makes every handler prove it at `register`. Ask for exactly what the use needs: moving to one thread is `Send`; sharing `&Bus` across threads would be `Sync`.

Syntax: `Vec<Box<dyn Handler + Send>>` · `trait Handler: Send { .. }` (supertrait form) · `Arc<AtomicUsize>` with `fetch_add(1, Ordering::Relaxed)`.""", "O(events × handlers)", "O(replies)"),
    "Would you put `Send` on the trait (`trait Handler: Send`) or on the object type? What does each choice cost other users of the trait?",
    ["Trait objects carry only the auto traits written on them.", "`Rc`/`Cell` → `Arc`/atomics to cross threads.", "Ask for `Send` or `Sync` according to how the value is used."],
    rules=dict(types=["Rc"], unsafe=True),
    related=("C1", "S7"),
    wrong=dict(
        handler_major_order=sub(BUS_SOLUTION, "        for e in events {\n            for h in &self.handlers {\n                out.extend(h.handle(e));\n            }\n        }",
                                "        for h in &self.handlers {\n            for e in events {\n                out.extend(h.handle(e));\n            }\n        }"),
        store_not_add=sub(BUS_SOLUTION, "self.count.fetch_add(1, Ordering::Relaxed);", "self.count.store(1, Ordering::Relaxed);"),
    ),
))

ENTITY_SOLUTION = r"""
use std::any::{Any, TypeId};

/// A piece of data attached to an entity. At most one of each type per entity.
pub trait Component: Any {
    fn name(&self) -> String;
}

#[derive(Default)]
pub struct Entity {
    components: Vec<Box<dyn Component>>,
}

impl Entity {
    fn position<T: Component>(&self) -> Option<usize> {
        // `(**c)` is the dyn Component, so type_id comes from its vtable: the concrete type.
        self.components.iter().position(|c| (**c).type_id() == TypeId::of::<T>())
    }

    /// Adds `c`, replacing a component of the same type in place. Returns the one it replaced.
    pub fn insert<T: Component>(&mut self, c: T) -> Option<T> {
        match self.position::<T>() {
            Some(i) => {
                let new: Box<dyn Component> = Box::new(c);
                let old: Box<dyn Any> = std::mem::replace(&mut self.components[i], new) as Box<dyn Component>;
                old.downcast::<T>().ok().map(|b| *b)
            }
            None => {
                self.components.push(Box::new(c));
                None
            }
        }
    }

    pub fn get<T: Component>(&self) -> Option<&T> {
        self.components.iter().find_map(|c| {
            let any: &dyn Any = &**c;
            any.downcast_ref::<T>()
        })
    }

    pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
        self.components.iter_mut().find_map(|c| {
            let any: &mut dyn Any = &mut **c;
            any.downcast_mut::<T>()
        })
    }

    pub fn remove<T: Component>(&mut self) -> Option<T> {
        let i = self.position::<T>()?;
        let b: Box<dyn Any> = self.components.remove(i);
        b.downcast::<T>().ok().map(|b| *b)
    }

    /// The components' names in insertion order.
    pub fn names(&self) -> Vec<String> {
        self.components.iter().map(|c| c.name()).collect()
    }
}
"""

ENTITY_TEST_TYPES = r"""
#[derive(Debug, PartialEq)]
struct Pos(i32, i32);
impl Component for Pos {
    fn name(&self) -> String {
        "pos".into()
    }
}

#[derive(Debug, PartialEq)]
struct Health(u32);
impl Component for Health {
    fn name(&self) -> String {
        format!("health {}", self.0)
    }
}

#[derive(Debug, PartialEq)]
struct Tag<T>(T);
impl<T: std::fmt::Debug + 'static> Component for Tag<T> {
    fn name(&self) -> String {
        format!("tag {:?}", self.0)
    }
}
"""

P.append(write(
    "any-downcasting", "Downcasting with Any", "medium", "static-vs-dynamic", ["Any", "TypeId", "downcast", "trait upcasting"],
    """
        An `Entity` holds at most one component of each type as `Box<dyn Component>`. Implement it:

        - `insert` adds a component, or replaces the one of the same type *in place* and returns the old value.
        - `get`, `get_mut` and `remove` find the component of type `T`; `remove` hands it back by value.
        - `names` lists `name()` of each component in insertion order.

        Components are defined by users of the crate (the tests define their own), so `Component` can't grow new
        required methods.
    """,
    r"""
    use std::any::Any;

    /// A piece of data attached to an entity. At most one of each type per entity.
    pub trait Component: Any {
        fn name(&self) -> String;
    }

    #[derive(Default)]
    pub struct Entity {
        components: Vec<Box<dyn Component>>,
    }

    impl Entity {
        /// Adds `c`, replacing a component of the same type in place. Returns the one it replaced.
        pub fn insert<T: Component>(&mut self, c: T) -> Option<T> {
            todo!()
        }

        pub fn get<T: Component>(&self) -> Option<&T> {
            todo!()
        }

        pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
            todo!()
        }

        pub fn remove<T: Component>(&mut self) -> Option<T> {
            todo!()
        }

        /// The components' names in insertion order.
        pub fn names(&self) -> Vec<String> {
            todo!()
        }
    }
    """,
    ENTITY_SOLUTION,
    [ENTITY_TEST_TYPES,
     T("insert_then_get", "insert Pos(1, 2); get::<Pos>()", "e.get::<Pos>()", "Some(&Pos(1, 2))", setup="let mut e = Entity::default();\ne.insert(Pos(1, 2));"),
     T("missing_type", "insert Pos; get::<Health>()", "e.get::<Health>()", "None", setup="let mut e = Entity::default();\ne.insert(Pos(0, 0));"),
     T("replace_returns_old", "insert Pos(1, 2), insert Pos(3, 4)", "(old, e.get::<Pos>(), e.names())", '(Some(Pos(1, 2)), Some(&Pos(3, 4)), vec!["pos".to_string()])',
       setup="let mut e = Entity::default();\ne.insert(Pos(1, 2));\nlet old = e.insert(Pos(3, 4));"),
     T("get_mut_edits", "get_mut::<Health>() then += 5", "e.get::<Health>()", "Some(&Health(15))",
       setup="let mut e = Entity::default();\ne.insert(Health(10));\ne.get_mut::<Health>().unwrap().0 += 5;"),
     T("remove_by_value", "insert Health(7), remove::<Health>()", "(e.remove::<Health>(), e.get::<Health>())", "(Some(Health(7)), None)",
       setup="let mut e = Entity::default();\ne.insert(Health(7));")],
    [ENTITY_TEST_TYPES,
     T("replace_keeps_position", "insert Pos, Health(1), Tag(\"a\"); replace Health(2)", "e.names()", 'vec!["pos", "health 2", "tag \\"a\\""]',
       setup='let mut e = Entity::default();\ne.insert(Pos(0, 0));\ne.insert(Health(1));\ne.insert(Tag("a"));\ne.insert(Health(2));'),
     T("remove_keeps_order", "insert Pos, Health, Tag(1u8); remove Health", "(e.names(), e.get::<Tag<u8>>())", '(vec!["pos".to_string(), "tag 1".to_string()], Some(&Tag(1u8)))',
       setup="let mut e = Entity::default();\ne.insert(Pos(0, 0));\ne.insert(Health(9));\ne.insert(Tag(1u8));\ne.remove::<Health>();"),
     T("remove_missing", "remove::<Pos>() on empty", "Entity::default().remove::<Pos>()", "None"),
     T("empty_names", "Entity::default().names()", "Entity::default().names()", "Vec::<String>::new()"),
     T("generic_types_are_distinct", "Tag(1u8) and Tag(1u16)", "(e.get::<Tag<u8>>(), e.get::<Tag<u16>>(), e.names().len())", "(Some(&Tag(1u8)), Some(&Tag(2u16)), 2)",
       setup="let mut e = Entity::default();\ne.insert(Tag(1u8));\ne.insert(Tag(2u16));"),
     T("insert_new_returns_none", "insert Pos into empty", "Entity::default().insert(Pos(5, 5))", "None"),
     T("get_mut_missing", "get_mut::<Pos>() on empty", "Entity::default().get_mut::<Pos>().is_none()", "true"),
     T("remove_then_insert_appends", "insert Pos, Health; remove Pos; insert Pos", "e.names()", 'vec!["health 3", "pos"]',
       setup="let mut e = Entity::default();\ne.insert(Pos(0, 0));\ne.insert(Health(3));\ne.remove::<Pos>();\ne.insert(Pos(1, 1));"),
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(4409);
         for _ in 0..200 {
             let mut e = Entity::default();
             let mut model: Vec<(usize, i32)> = Vec::new();
             let mut log = Vec::new();
             for _ in 0..10 {
                 let kind = rng.below(3);
                 let v = rng.int(0, 9) as i32;
                 let pos = model.iter().position(|&(k, _)| k == kind);
                 if rng.below(3) == 0 {
                     log.push(format!("remove {kind}"));
                     let got = match kind {
                         0 => e.remove::<Pos>().map(|p| p.0),
                         1 => e.remove::<Health>().map(|h| h.0 as i32),
                         _ => e.remove::<Tag<i32>>().map(|t| t.0),
                     };
                     let want = pos.map(|i| model.remove(i).1);
                     check!(log.join(", "), got, want);
                 } else {
                     log.push(format!("insert {kind}={v}"));
                     let got = match kind {
                         0 => e.insert(Pos(v, 0)).map(|p| p.0),
                         1 => e.insert(Health(v as u32)).map(|h| h.0 as i32),
                         _ => e.insert(Tag(v)).map(|t| t.0),
                     };
                     let want = match pos {
                         Some(i) => Some(std::mem::replace(&mut model[i].1, v)),
                         None => {
                             model.push((kind, v));
                             None
                         }
                     };
                     check!(log.join(", "), got, want);
                 }
                 let want_names: Vec<String> = model.iter().map(|&(k, v)| match k { 0 => "pos".to_string(), 1 => format!("health {v}"), _ => format!("tag {v}") }).collect();
                 check!(log.join(", "), e.names(), want_names);
             }
         }
     }
     """],
    [("rust", "`Component: Any` makes `type_id` part of every `dyn Component` vtable. Since Rust 1.86 a `&dyn Component` also upcasts to `&dyn Any` (and `Box<dyn Component>` to `Box<dyn Any>`), which has `downcast_ref`, `downcast_mut` and `downcast`."),
     ("edge case", "Call `type_id` on the trait object, `(**c).type_id()`, not on `c`: `c` is a `&Box<dyn Component>`, and `Box<dyn Component>` is itself `Any`, so you'd get the Box's type id and match nothing.")],
    ("""`Any` gives a `'static` type a runtime `TypeId`; downcasting compares it with `TypeId::of::<T>()` and casts on a match. With `Any` as a supertrait, the object can be upcast (`&dyn Component` → `&dyn Any`, stable since 1.86); before that, the usual trick was a required `fn as_any(&self) -> &dyn Any`. The classic bug is taking `type_id` of the smart pointer instead of the object.

Syntax: `let a: &dyn Any = &**c; a.downcast_ref::<T>()` · `let b: Box<dyn Any> = boxed; b.downcast::<T>()` → `Result<Box<T>, Box<dyn Any>>`.""", "O(components) per call", "O(components)"),
    "An ECS with thousands of entities wouldn't scan a Vec of boxes. What layout would you use, and where would `TypeId` still appear?",
    ["`trait Component: Any` + upcasting to `dyn Any` enables downcasts.", "Call `type_id` on the object, not on the Box.", "`Box<dyn Any>::downcast` returns the value by value."],
    related=("S7", "S11"),
    wrong=dict(
        type_id_of_the_box=sub(ENTITY_SOLUTION, "(**c).type_id()", "c.type_id()"),
        replace_appends=sub(ENTITY_SOLUTION, "                let new: Box<dyn Component> = Box::new(c);\n                let old: Box<dyn Any> = std::mem::replace(&mut self.components[i], new) as Box<dyn Component>;\n                old.downcast::<T>().ok().map(|b| *b)",
                            "                let new: Box<dyn Component> = Box::new(c);\n                let old: Box<dyn Any> = self.components.remove(i);\n                self.components.push(new);\n                old.downcast::<T>().ok().map(|b| *b)"),
    ),
))

LOGGER_HEAD = r"""
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

pub trait Logger {
    fn log(&self, level: Level, msg: &str);

    /// Pushes out anything buffered. Does nothing by default; every wrapper must pass it on.
    fn flush(&self) {}
}

/// A logger that can be shared across threads.
pub type BoxLogger = Box<dyn Logger + Send + Sync>;
"""

LOGGER_SOLUTION = LOGGER_HEAD + r"""
/// Prepends `prefix` to every message.
pub struct PrefixLogger {
    prefix: String,
    inner: BoxLogger,
}

impl PrefixLogger {
    pub fn new(prefix: &str, inner: BoxLogger) -> Self {
        PrefixLogger { prefix: prefix.to_string(), inner }
    }
}

impl Logger for PrefixLogger {
    fn log(&self, level: Level, msg: &str) {
        self.inner.log(level, &format!("{}{}", self.prefix, msg));
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Drops messages below `min`.
pub struct LevelFilter {
    min: Level,
    inner: BoxLogger,
}

impl LevelFilter {
    pub fn new(min: Level, inner: BoxLogger) -> Self {
        LevelFilter { min, inner }
    }
}

impl Logger for LevelFilter {
    fn log(&self, level: Level, msg: &str) {
        if level >= self.min {
            self.inner.log(level, msg);
        }
    }

    fn flush(&self) {
        self.inner.flush();
    }
}

/// Sends every message to all its sinks, in order.
pub struct Tee {
    sinks: Vec<BoxLogger>,
}

impl Tee {
    pub fn new(sinks: Vec<BoxLogger>) -> Self {
        Tee { sinks }
    }
}

impl Logger for Tee {
    fn log(&self, level: Level, msg: &str) {
        for s in &self.sinks {
            s.log(level, msg);
        }
    }

    fn flush(&self) {
        for s in &self.sinks {
            s.flush();
        }
    }
}

/// Lets one logger be shared by several wrappers.
impl<L: Logger + ?Sized> Logger for Arc<L> {
    fn log(&self, level: Level, msg: &str) {
        (**self).log(level, msg);
    }

    fn flush(&self) {
        (**self).flush();
    }
}
"""

LOGGER_REC = r"""
use std::sync::{Arc, Mutex};

struct Rec(Mutex<Vec<String>>);

impl Logger for Rec {
    fn log(&self, level: Level, msg: &str) {
        self.0.lock().unwrap().push(format!("{level:?} {msg}"));
    }
    fn flush(&self) {
        self.0.lock().unwrap().push("flush".into());
    }
}

fn rec() -> Arc<Rec> {
    Arc::new(Rec(Mutex::new(Vec::new())))
}

fn lines(r: &Rec) -> Vec<String> {
    r.0.lock().unwrap().clone()
}
"""

P.append(write(
    "plugin-logger", "Plugin logger with trait objects", "medium", "static-vs-dynamic", ["dyn Trait", "decorator", "Send + Sync", "forwarding impls"],
    """
        Build composable loggers over `Box<dyn Logger + Send + Sync>`:

        - `PrefixLogger` prepends its prefix to each message and passes it on.
        - `LevelFilter` passes on only messages at or above `min`.
        - `Tee` sends each message to all its sinks, in order.
        - `Arc<L>` is a `Logger` too, so one sink can sit under several wrappers.

        `flush` has an empty default, but every wrapper must pass it on. Loggers get shared across threads.
    """,
    r"""
    use std::sync::Arc;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Level {
        Debug,
        Info,
        Warn,
        Error,
    }

    pub trait Logger {
        fn log(&self, level: Level, msg: &str);

        /// Pushes out anything buffered. Does nothing by default; every wrapper must pass it on.
        fn flush(&self) {}
    }

    /// A logger that can be shared across threads.
    pub type BoxLogger = Box<dyn Logger + Send + Sync>;

    /// Prepends `prefix` to every message.
    pub struct PrefixLogger {
        // TODO
    }

    impl PrefixLogger {
        pub fn new(prefix: &str, inner: BoxLogger) -> Self {
            todo!()
        }
    }

    impl Logger for PrefixLogger {
        fn log(&self, level: Level, msg: &str) {
            todo!()
        }
    }

    /// Drops messages below `min`.
    pub struct LevelFilter {
        // TODO
    }

    impl LevelFilter {
        pub fn new(min: Level, inner: BoxLogger) -> Self {
            todo!()
        }
    }

    impl Logger for LevelFilter {
        fn log(&self, level: Level, msg: &str) {
            todo!()
        }
    }

    /// Sends every message to all its sinks, in order.
    pub struct Tee {
        // TODO
    }

    impl Tee {
        pub fn new(sinks: Vec<BoxLogger>) -> Self {
            todo!()
        }
    }

    impl Logger for Tee {
        fn log(&self, level: Level, msg: &str) {
            todo!()
        }
    }

    /// Lets one logger be shared by several wrappers.
    impl<L: Logger + ?Sized> Logger for Arc<L> {
        fn log(&self, level: Level, msg: &str) {
            todo!()
        }
    }
    """,
    LOGGER_SOLUTION,
    [LOGGER_REC,
     T("prefix", "PrefixLogger(\"[db] \") → rec; Info \"up\"", "lines(&r)", 'vec!["Info [db] up"]',
       setup='let r = rec();\nPrefixLogger::new("[db] ", Box::new(r.clone())).log(Level::Info, "up");'),
     T("filter_at_or_above", "LevelFilter(Warn) → rec; Info a, Warn b, Error c", "lines(&r)", 'vec!["Warn b", "Error c"]',
       setup='let r = rec();\nlet f = LevelFilter::new(Level::Warn, Box::new(r.clone()));\nf.log(Level::Info, "a");\nf.log(Level::Warn, "b");\nf.log(Level::Error, "c");'),
     T("nested", "Prefix(\"a:\", Tee[r1, Prefix(\"b:\", r2)]); Info x", "(lines(&r1), lines(&r2))", '(vec!["Info a:x".to_string()], vec!["Info b:a:x".to_string()])',
       setup='let (r1, r2) = (rec(), rec());\nlet l = PrefixLogger::new("a:", Box::new(Tee::new(vec![Box::new(r1.clone()), Box::new(PrefixLogger::new("b:", Box::new(r2.clone())))])));\nl.log(Level::Info, "x");'),
     T("flush_passes_through", "Prefix(Filter(Tee[r1, r2])).flush()", "(lines(&r1), lines(&r2))", '(vec!["flush".to_string()], vec!["flush".to_string()])',
       setup='let (r1, r2) = (rec(), rec());\nlet l = PrefixLogger::new("p", Box::new(LevelFilter::new(Level::Error, Box::new(Tee::new(vec![Box::new(r1.clone()), Box::new(r2.clone())])))));\nl.flush();'),
     """
     #[test]
     fn shared_across_threads() {
         let r = rec();
         let root: Arc<dyn Logger + Send + Sync> = Arc::new(Tee::new(vec![Box::new(PrefixLogger::new("t", Box::new(r.clone())))]));
         std::thread::scope(|s| {
             for i in 0..4 {
                 let root = &root;
                 s.spawn(move || root.log(Level::Info, &i.to_string()));
             }
         });
         let mut got = lines(&r);
         got.sort();
         check!("4 threads log through one Tee", got, vec!["Info t0", "Info t1", "Info t2", "Info t3"]);
     }
     """],
    [LOGGER_REC,
     T("filter_error_only", "LevelFilter(Error); Debug, Info, Warn", "lines(&r)", "Vec::<String>::new()",
       setup='let r = rec();\nlet f = LevelFilter::new(Level::Error, Box::new(r.clone()));\nf.log(Level::Debug, "a");\nf.log(Level::Info, "b");\nf.log(Level::Warn, "c");'),
     T("filter_debug_passes_all", "LevelFilter(Debug); Debug d", "lines(&r)", 'vec!["Debug d"]',
       setup='let r = rec();\nLevelFilter::new(Level::Debug, Box::new(r.clone())).log(Level::Debug, "d");'),
     T("nested_filters", "Filter(Info, Filter(Error)); Warn w, Error e", "lines(&r)", 'vec!["Error e"]',
       setup='let r = rec();\nlet l = LevelFilter::new(Level::Info, Box::new(LevelFilter::new(Level::Error, Box::new(r.clone()))));\nl.log(Level::Warn, "w");\nl.log(Level::Error, "e");'),
     T("tee_order", "Tee[Prefix(\"1\", r), Prefix(\"2\", r)]; Warn x", "lines(&r)", 'vec!["Warn 1x", "Warn 2x"]',
       setup='let r = rec();\nTee::new(vec![Box::new(PrefixLogger::new("1", Box::new(r.clone()))), Box::new(PrefixLogger::new("2", Box::new(r.clone())))]).log(Level::Warn, "x");'),
     T("empty_tee", "Tee[] log and flush", "true", "true", setup='let t = Tee::new(vec![]);\nt.log(Level::Info, "x");\nt.flush();'),
     T("arc_forwards_flush", "shared Arc<Tee[r]> under a Prefix; flush", "lines(&r)", 'vec!["flush"]',
       setup='let r = rec();\nlet shared = Arc::new(Tee::new(vec![Box::new(r.clone())]));\nlet l = PrefixLogger::new("x", Box::new(Arc::clone(&shared)));\nl.flush();'),
     T("filter_forwards_flush", "LevelFilter(Error) → rec; flush", "lines(&r)", 'vec!["flush"]',
       setup='let r = rec();\nLevelFilter::new(Level::Error, Box::new(r.clone())).flush();'),
     T("unicode_prefix", "Prefix(\"» \"); Error é", "lines(&r)", 'vec!["Error » é"]',
       setup='let r = rec();\nPrefixLogger::new("» ", Box::new(r.clone())).log(Level::Error, "é");'),
     """
     #[test]
     fn default_flush_sink() {
         // A sink that keeps the default flush: flushing a Tee around it must not fail.
         struct Quiet;
         impl Logger for Quiet {
             fn log(&self, _: Level, _: &str) {}
         }
         let r = rec();
         let t = Tee::new(vec![Box::new(Quiet), Box::new(r.clone())]);
         t.flush();
         check!("Tee[Quiet, r].flush()", lines(&r), vec!["flush"]);
     }
     """,
     r"""
     #[test]
     fn random_chains_vs_model() {
         let mut rng = anneal_prelude::Rng::new(4410);
         let levels = [Level::Debug, Level::Info, Level::Warn, Level::Error];
         for _ in 0..200 {
             // Outermost first: Some(prefix) or None + a filter level.
             let n = rng.below(5);
             let chain: Vec<(Option<String>, Level)> = (0..n)
                 .map(|_| if rng.bool() { let len = rng.below(3); (Some(rng.string(len, "ab")), Level::Debug) } else { (None, *rng.pick(&levels)) })
                 .collect();
             let r = rec();
             let mut l: BoxLogger = Box::new(r.clone());
             for (p, min) in chain.iter().rev() {
                 l = match p {
                     Some(p) => Box::new(PrefixLogger::new(p, l)),
                     None => Box::new(LevelFilter::new(*min, l)),
                 };
             }
             let mut want = Vec::new();
             for i in 0..3 {
                 let level = *rng.pick(&levels);
                 let msg = format!("m{i}");
                 l.log(level, &msg);
                 let mut text = msg;
                 let mut pass = true;
                 for (p, min) in &chain {
                     match p {
                         Some(p) => text = format!("{p}{text}"),
                         None => pass &= level >= *min,
                     }
                 }
                 if pass {
                     want.push(format!("{level:?} {text}"));
                 }
             }
             check!(format!("chain = {chain:?}"), lines(&r), want);
         }
     }
     """],
    [("approach", "Each wrapper owns a `BoxLogger` and does its one job before delegating. `Tee` owns a `Vec<BoxLogger>`."),
     ("rust", "Store exactly `BoxLogger`: a field of `Box<dyn Logger>` drops the `Send + Sync` promise, and then `Arc<Tee>` can't cross threads. In the `Arc<L>` impl, `?Sized` lets `L` be `dyn Logger + Send + Sync`."),
     ("edge case", "The default `flush` does nothing, so any wrapper that doesn't override it silently swallows flushes. That includes `Arc<L>`.")],
    ("""A decorator owns a `Box<dyn Trait>` and implements the same trait, so wrappers nest freely and each only knows its own job. A default method on the trait is a trap for wrappers: they must override it to forward, or the call stops there. The forwarding impl for `Arc<L>` with `L: ?Sized` makes shared sinks and `Arc<dyn Logger>` usable wherever a logger is.

Syntax: `type BoxLogger = Box<dyn Logger + Send + Sync>;` · `impl<L: Logger + ?Sized> Logger for Arc<L> { fn log(&self, l: Level, m: &str) { (**self).log(l, m) } fn flush(&self) { (**self).flush() } }`.""", "O(depth) per message", "O(wrappers)"),
    "`Tee` could be generic, `Tee<A: Logger, B: Logger>`. When would you pick that over `Vec<Box<dyn Logger>>`?",
    ["Decorators over `Box<dyn Trait>` compose at run time.", "Wrappers must forward every default method they don't want swallowed.", "Keep `+ Send + Sync` on the stored object type."],
    related=("L5", "C1"),
    wrong=dict(
        filter_strictly_above=sub(LOGGER_SOLUTION, "if level >= self.min {", "if level > self.min {"),
        tee_swallows_flush=sub(LOGGER_SOLUTION, "\n    fn flush(&self) {\n        for s in &self.sinks {\n            s.flush();\n        }\n    }\n", "\n"),
        arc_swallows_flush=sub(LOGGER_SOLUTION, "\n    fn flush(&self) {\n        (**self).flush();\n    }\n", "\n"),
    ),
))

# ---------------------------------------------------------------- object safety (medium)

PLAYER_TRAIT = r"""
pub trait Player {
    fn new(name: &str, score: u32) -> Self;
    fn name(&self) -> String;
    fn score(&self) -> u32;

    /// True if `self` ranks above `other`: a higher score, or the same score and a name that sorts first.
    fn beats(&self, other: &Self) -> bool {
        self.score() > other.score() || (self.score() == other.score() && self.name() < other.name())
    }
}
"""

PLAYER_REST = r"""
pub struct Human {
    pub name: String,
    pub score: u32,
}

/// A team's name is its members joined with "+".
pub struct Team {
    pub members: Vec<String>,
    pub score: u32,
}

impl Player for Human {
    fn new(name: &str, score: u32) -> Self {
        Human { name: name.to_string(), score }
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn score(&self) -> u32 {
        self.score
    }
}

impl Player for Team {
    fn new(name: &str, score: u32) -> Self {
        Team { members: name.split('+').map(String::from).collect(), score }
    }

    fn name(&self) -> String {
        self.members.join("+")
    }

    fn score(&self) -> u32 {
        self.score
    }
}

/// The player nobody beats; the first of them on an exact tie. None if there are no players.
pub fn winner(players: &[Box<dyn Player>]) -> Option<&dyn Player> {
    let mut best: Option<&dyn Player> = None;
    for p in players {
        if best.map_or(true, |b| p.beats(b)) {
            best = Some(p.as_ref());
        }
    }
    best
}
"""

PLAYER_FIXED = (PLAYER_TRAIT.replace("fn new(name: &str, score: u32) -> Self;", "fn new(name: &str, score: u32) -> Self\n    where\n        Self: Sized;")
                .replace("other: &Self", "other: &dyn Player"))

P.append(fix(
    "fix-self-in-argument", "Fix: Self in a trait that must be dyn (E0038)", "medium", "object-safety", ["E0038", "dyn compatibility", "where Self: Sized"],
    """
        `winner` doesn't compile: `Player` is not dyn-compatible, for two different reasons. Fix the trait so that
        `winner` works and players of different types can be compared (`human.beats(&team)`). Keep `new`.
    """,
    PLAYER_TRAIT + PLAYER_REST,
    PLAYER_FIXED + PLAYER_REST,
    [T("winner_mixed", "Human ann 5, Team a+b 7, Human bo 6", "winner(&ps).map(|p| p.name())", 'Some("a+b".to_string())',
       setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("ann", 5)), Box::new(Team::new("a+b", 7)), Box::new(Human::new("bo", 6))];'),
     T("beats_across_types", "Human zed 3 beats Team x+y 2", 'Human::new("zed", 3).beats(&Team::new("x+y", 2))', "true"),
     T("tie_broken_by_name", "Team b+c 4 vs Human al 4", '(Team::new("b+c", 4).beats(&Human::new("al", 4)), Human::new("al", 4).beats(&Team::new("b+c", 4)))', "(false, true)"),
     T("exact_tie_first_wins", "two Humans \"x\" 1", "std::ptr::addr_eq(winner(&ps).unwrap(), ps[0].as_ref())", "true",
       setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("x", 1)), Box::new(Human::new("x", 1))];'),
     T("no_players", "winner(&[])", "winner(&[]).is_none()", "true")],
    [T("never_beats_itself", "h.beats(&h)", "h.beats(&h)", "false", setup='let h = Human::new("a", 1);'),
     T("team_new_splits", "Team::new(\"p+q+r\", 0).members", 'Team::new("p+q+r", 0).members', 'vec!["p", "q", "r"]'),
     T("single_player", "one Team", "winner(&ps).map(|p| p.score())", "Some(9)", setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Team::new("solo", 9))];'),
     T("winner_by_name", "scores all 2: dan, b+z, cat", "winner(&ps).map(|p| p.name())", 'Some("b+z".to_string())',
       setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("dan", 2)), Box::new(Team::new("b+z", 2)), Box::new(Human::new("cat", 2))];'),
     T("winner_last", "scores 1, 2, 3", "winner(&ps).map(|p| p.name())", 'Some("c".to_string())',
       setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("a", 1)), Box::new(Human::new("b", 2)), Box::new(Human::new("c", 3))];'),
     T("zero_scores", "scores 0, 0", "winner(&ps).map(|p| p.name())", 'Some("m".to_string())',
       setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("m", 0)), Box::new(Human::new("n", 0))];'),
     T("dyn_beats_dyn", "ps[0].beats(ps[1].as_ref())", "ps[0].beats(ps[1].as_ref())", "true",
       setup='let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("a", u32::MAX)), Box::new(Team::new("b", 0))];'),
     """
     #[test]
     fn a_new_player_type() {
         // Constructors stay available on concrete types.
         struct Bot(u32);
         impl Player for Bot {
             fn new(_name: &str, score: u32) -> Self {
                 Bot(score)
             }
             fn name(&self) -> String {
                 format!("bot{}", self.0)
             }
             fn score(&self) -> u32 {
                 self.0
             }
         }
         let ps: Vec<Box<dyn Player>> = vec![Box::new(Human::new("h", 4)), Box::new(Bot::new("", 4))];
         check!("Human h 4, Bot 4", winner(&ps).map(|p| p.name()), Some("bot4".to_string()));
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4411);
         for _ in 0..300 {
             let n = rng.below(6);
             let specs: Vec<(String, u32, bool)> = (0..n).map(|_| (rng.string(1, "ab"), rng.below(3) as u32, rng.bool())).collect();
             let ps: Vec<Box<dyn Player>> = specs
                 .iter()
                 .map(|(name, score, team)| if *team { Box::new(Team::new(name, *score)) as Box<dyn Player> } else { Box::new(Human::new(name, *score)) })
                 .collect();
             let mut best: Option<usize> = None;
             for (i, (name, score, _)) in specs.iter().enumerate() {
                 if best.map_or(true, |b| *score > specs[b].1 || (*score == specs[b].1 && *name < specs[b].0)) {
                     best = Some(i);
                 }
             }
             let got = winner(&ps).map(|w| ps.iter().position(|p| std::ptr::addr_eq(p.as_ref(), w)).unwrap());
             check!(format!("players = {specs:?}"), got, best);
         }
     }
     """],
    [("approach", "Read both notes under E0038. `new` returns `Self` with no receiver, so it can't go in a vtable, and no caller of `dyn Player` needs it. `beats` takes `&Self`, which for `dyn Player` would have to be the same hidden type."),
     ("rust", "`where Self: Sized` on `new` keeps it for concrete types and leaves it out of `dyn Player`. That escape doesn't work for `beats`, since `winner` calls it on trait objects; take `other: &dyn Player` instead.")],
    ("""A trait is dyn-compatible when every method can be called through a vtable without knowing the concrete type. Associated functions without `self`, `Self` in argument or return position, and generic methods break that. Mark a method `where Self: Sized` when trait objects don't need it; change its signature (`&Self` → `&dyn Player`) when they do. A side effect: `&Self` also forbade comparing a `Human` with a `Team`.

Syntax: `fn new(name: &str, score: u32) -> Self where Self: Sized;` · `fn beats(&self, other: &dyn Player) -> bool`.""", "O(n)", "O(1)"),
    "`PartialOrd` uses `&Rhs` with `Rhs = Self`. How would you sort a `Vec<Box<dyn Player>>` without a trait object implementing `Ord`?",
    ["Methods without a receiver, or with `Self` in arguments, break dyn compatibility.", "`where Self: Sized` removes a method from the vtable; change the signature if trait objects need it."],
    rules=dict(lines=6),
    related=("L5",),
    wrong=dict(
        ties_go_to_self=sub(PLAYER_FIXED, "self.score() == other.score() && self.name() < other.name()", "self.score() == other.score() && self.name() <= other.name()") + PLAYER_REST,
        last_on_tie=PLAYER_FIXED + sub(PLAYER_REST, "best.map_or(true, |b| p.beats(b))", "best.map_or(true, |b| !b.beats(p.as_ref()))"),
    ),
))

STORE_STARTER = r"""
use std::collections::BTreeMap;
use std::fmt::Display;

pub trait Store {
    fn get(&self, key: &str) -> Option<String>;
    /// Stores `value` formatted with Display.
    fn put<V: Display>(&mut self, key: &str, value: V);
    /// The keys in sorted order.
    fn keys(&self) -> impl Iterator<Item = &str>;
}

#[derive(Default)]
pub struct MemStore {
    map: BTreeMap<String, String>,
}

impl Store for MemStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }

    fn put<V: Display>(&mut self, key: &str, value: V) {
        self.map.insert(key.to_string(), value.to_string());
    }

    fn keys(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

/// Stores values uppercased.
#[derive(Default)]
pub struct UpperStore {
    map: BTreeMap<String, String>,
}

impl Store for UpperStore {
    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }

    fn put<V: Display>(&mut self, key: &str, value: V) {
        self.map.insert(key.to_string(), value.to_string().to_uppercase());
    }

    fn keys(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

/// A view of `inner` under a key prefix: key "k" here is "<prefix>k" in `inner`.
pub struct Prefixed {
    pub prefix: String,
    pub inner: Box<dyn Store>,
}

impl Store for Prefixed {
    fn get(&self, key: &str) -> Option<String> {
        self.inner.get(&format!("{}{}", self.prefix, key))
    }

    fn put<V: Display>(&mut self, key: &str, value: V) {
        self.inner.put(&format!("{}{}", self.prefix, key), value);
    }

    /// Only the inner keys under the prefix, with the prefix removed.
    fn keys(&self) -> impl Iterator<Item = &str> {
        self.inner.keys().filter_map(|k| k.strip_prefix(self.prefix.as_str()))
    }
}

/// One of each basic store.
pub fn all_stores() -> Vec<Box<dyn Store>> {
    vec![Box::new(MemStore::default()), Box::new(UpperStore::default())]
}
"""

STORE_SOLUTION = (STORE_STARTER
                  .replace("fn put<V: Display>(&mut self, key: &str, value: V)", "fn put(&mut self, key: &str, value: &dyn Display)")
                  .replace("fn keys(&self) -> impl Iterator<Item = &str>;", "fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_>;")
                  .replace("fn keys(&self) -> impl Iterator<Item = &str> {\n        self.map.keys().map(String::as_str)",
                           "fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_> {\n        Box::new(self.map.keys().map(String::as_str))")
                  .replace("fn keys(&self) -> impl Iterator<Item = &str> {\n        self.inner.keys().filter_map(|k| k.strip_prefix(self.prefix.as_str()))",
                           "fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_> {\n        Box::new(self.inner.keys().filter_map(move |k| k.strip_prefix(self.prefix.as_str())))"))
assert "impl Iterator" not in STORE_SOLUTION and "<V" not in STORE_SOLUTION

P.append(fix(
    "fix-generic-method-dyn", "Fix: generic and impl Trait methods in a dyn trait (E0038)", "medium", "object-safety", ["E0038", "generic methods", "RPITIT", "&dyn Display"],
    """
        Nothing here compiles: `Store` is used as `dyn Store`, but it isn't dyn-compatible. Fix it so every store
        works behind `Box<dyn Store>`. Callers pass values by reference, any `Display` type: `store.put("n", &5)`.
    """,
    STORE_STARTER,
    STORE_SOLUTION,
    [T("put_and_get_each_store", "for each of all_stores(): put(\"n\", &5), put(\"s\", &\"hi\")", "got", 'vec![(Some("5".to_string()), Some("hi".to_string())), (Some("5".to_string()), Some("HI".to_string()))]',
       setup='let mut got = Vec::new();\nfor mut s in all_stores() {\n    s.put("n", &5);\n    s.put("s", &"hi");\n    got.push((s.get("n"), s.get("s")));\n}'),
     T("keys_sorted", "put b, a, c", "s.keys().collect::<Vec<_>>()", 'vec!["a", "b", "c"]',
       setup='let mut s: Box<dyn Store> = Box::new(MemStore::default());\nfor k in ["b", "a", "c"] {\n    s.put(k, &1);\n}'),
     T("prefixed_view", "Prefixed(\"user:\") over MemStore; put name=ann", '(p.get("name"), p.inner.get("user:name"))', '(Some("ann".to_string()), Some("ann".to_string()))',
       setup='let mut p = Prefixed { prefix: "user:".into(), inner: Box::new(MemStore::default()) };\np.put("name", &"ann");'),
     T("prefixed_keys", "inner keys user:a, user:b, zzz", "p.keys().collect::<Vec<_>>()", 'vec!["a", "b"]',
       setup='let mut inner = MemStore::default();\nfor k in ["user:b", "zzz", "user:a"] {\n    inner.put(k, &0);\n}\nlet p = Prefixed { prefix: "user:".into(), inner: Box::new(inner) };'),
     T("missing_key", "get(\"x\") on an empty store", 'MemStore::default().get("x")', "None")],
    [T("floats_and_chars", "put(\"f\", &2.5), put(\"c\", &'é') in UpperStore", '(s.get("f"), s.get("c"))', '(Some("2.5".to_string()), Some("É".to_string()))',
       setup="let mut s: Box<dyn Store> = Box::new(UpperStore::default());\ns.put(\"f\", &2.5);\ns.put(\"c\", &'é');"),
     T("overwrite", "put k=1 then k=2", '(s.get("k"), s.keys().count())', '(Some("2".to_string()), 1)',
       setup='let mut s: Box<dyn Store> = Box::new(MemStore::default());\ns.put("k", &1);\ns.put("k", &2);'),
     T("upper_keeps_keys", "UpperStore put(\"key\", &\"v\")", "(s.keys().collect::<Vec<_>>(), s.get(\"key\"))", '(vec!["key"], Some("V".to_string()))',
       setup='let mut s = UpperStore::default();\ns.put("key", &"v");'),
     T("empty_keys", "MemStore::default().keys()", "MemStore::default().keys().count()", "0"),
     T("prefixed_over_upper", "Prefixed(\"p/\") over UpperStore; put x=abc", '(p.get("x"), p.inner.keys().collect::<Vec<_>>())', '(Some("ABC".to_string()), vec!["p/x"])',
       setup='let mut p = Prefixed { prefix: "p/".into(), inner: Box::new(UpperStore::default()) };\np.put("x", &"abc");'),
     T("nested_prefixed", "Prefixed(\"a/\", Prefixed(\"b/\", MemStore)); put k", '(p.get("k"), p.keys().collect::<Vec<_>>())', '(Some("1".to_string()), vec!["k"])',
       setup='let inner = Prefixed { prefix: "b/".into(), inner: Box::new(MemStore::default()) };\nlet mut p = Prefixed { prefix: "a/".into(), inner: Box::new(inner) };\np.put("k", &1);'),
     T("empty_prefix", "Prefixed(\"\") sees every key", "p.keys().collect::<Vec<_>>()", 'vec!["x", "y"]',
       setup='let mut inner = MemStore::default();\ninner.put("y", &0);\ninner.put("x", &0);\nlet p = Prefixed { prefix: String::new(), inner: Box::new(inner) };'),
     """
     #[test]
     fn user_display_type() {
         struct Point(i32, i32);
         impl std::fmt::Display for Point {
             fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                 write!(f, "({}, {})", self.0, self.1)
             }
         }
         let mut s: Box<dyn Store> = Box::new(MemStore::default());
         s.put("p", &Point(1, -2));
         check!("put(\\"p\\", &Point(1, -2))", s.get("p"), Some("(1, -2)".to_string()));
     }
     """,
     r"""
     #[test]
     fn random_vs_model() {
         let mut rng = anneal_prelude::Rng::new(4412);
         for _ in 0..200 {
             let mut p = Prefixed { prefix: "u:".into(), inner: Box::new(MemStore::default()) };
             let mut model = std::collections::BTreeMap::new();
             for _ in 0..8 {
                 let k = rng.string(1, "abc");
                 let v = rng.int(0, 99);
                 p.put(&k, &v);
                 model.insert(k, v.to_string());
             }
             let keys: Vec<String> = p.keys().map(String::from).collect();
             check!(format!("model = {model:?}"), keys, model.keys().cloned().collect::<Vec<_>>());
             for (k, v) in &model {
                 check!(format!("get({k:?})"), p.get(k), Some(v.clone()));
             }
         }
     }
     """],
    [("approach", "The compiler lists two reasons: a generic method (`put`) and a method returning `impl Trait` (`keys`). Both mean one method per concrete type, which a vtable can't hold."),
     ("rust", "Take the value as `&dyn Display`, and return `Box<dyn Iterator<Item = &str> + '_>`. The `'_` ties the iterator to `&self`; in `Prefixed`, the closure borrows `self.prefix`, so it needs `move` to capture `self` by reference."),
     ("edge case", "`where Self: Sized` would make the trait compile, but then `put` and `keys` couldn't be called on `Box<dyn Store>` at all.")],
    ("""Every method of a dyn-compatible trait needs exactly one vtable entry. A generic method has one instantiation per type argument, and `-> impl Trait` in a trait (RPITIT) has one hidden type per impl, so neither fits. Type-erase them: generic arguments become `&dyn Trait`, returned `impl Trait` becomes `Box<dyn Trait + '_>`. The cost is a heap allocation per `keys` call and a vtable call per item.

Syntax: `fn put(&mut self, key: &str, value: &dyn Display);` · `fn keys(&self) -> Box<dyn Iterator<Item = &str> + '_>;`.""", "O(log n) get/put; O(n) keys", "O(1) extra"),
    "How would you keep a zero-cost generic `put<V: Display>` for concrete stores while `dyn Store` still works? (Think extension traits.)",
    ["Generic methods and `-> impl Trait` methods aren't dyn-compatible.", "Erase them: `&dyn Trait` arguments, `Box<dyn Trait + '_>` returns."],
    related=("L5", "S6"),
    wrong=dict(
        upper_keys_too=sub(STORE_SOLUTION, "self.map.insert(key.to_string(), value.to_string().to_uppercase());", "self.map.insert(key.to_uppercase(), value.to_string().to_uppercase());"),
        prefixed_keys_unstripped=sub(STORE_SOLUTION, "Box::new(self.inner.keys().filter_map(move |k| k.strip_prefix(self.prefix.as_str())))",
                                     "Box::new(self.inner.keys().filter(move |k| k.starts_with(self.prefix.as_str())))"),
    ),
))

CLONE_HEAD = r"""
#[derive(Clone, Debug, PartialEq)]
pub struct Circle {
    pub r: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rect {
    pub w: f64,
    pub h: f64,
}
"""

CLONE_IMPLS = r"""
impl Shape for Circle {
    fn area(&self) -> f64 {
        std::f64::consts::PI * self.r * self.r
    }

    fn name(&self) -> String {
        format!("circle({})", self.r)
    }

    fn scaled(&self, k: f64) -> Self {
        Circle { r: self.r * k }
    }
}

impl Shape for Rect {
    fn area(&self) -> f64 {
        self.w * self.h
    }

    fn name(&self) -> String {
        format!("rect({}x{})", self.w, self.h)
    }

    fn scaled(&self, k: f64) -> Self {
        Rect { w: self.w * k, h: self.h * k }
    }
}
"""

CLONE_FNS = r"""
/// A copy of the scene that later edits to `scene` don't affect.
pub fn snapshot(scene: &Vec<Box<dyn Shape>>) -> Vec<Box<dyn Shape>> {
    scene.clone()
}

/// Every shape scaled by `k`.
pub fn scale_all(scene: &[Box<dyn Shape>], k: f64) -> Vec<Box<dyn Shape>> {
    scene.iter().map(|s| s.scaled_box(k)).collect()
}
"""

CLONE_TRAIT_SOLUTION = r"""
pub trait Shape: DynShape {
    fn area(&self) -> f64;
    fn name(&self) -> String;

    /// A copy scaled by `k` (every length times k).
    fn scaled(&self, k: f64) -> Self
    where
        Self: Sized;
}

/// The dyn-compatible half, written once for every `Clone` shape by the blanket impl below.
pub trait DynShape {
    fn clone_box(&self) -> Box<dyn Shape>;
    fn scaled_box(&self, k: f64) -> Box<dyn Shape>;
}

impl<T: Shape + Clone + 'static> DynShape for T {
    fn clone_box(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }

    fn scaled_box(&self, k: f64) -> Box<dyn Shape> {
        Box::new(self.scaled(k))
    }
}

impl Clone for Box<dyn Shape> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}
"""

CLONE_SOLUTION = CLONE_TRAIT_SOLUTION + CLONE_HEAD + CLONE_IMPLS + CLONE_FNS

P.append(fix(
    "where-self-sized", "Cloning trait objects: where Self: Sized and clone_box", "medium", "object-safety", ["E0038", "where Self: Sized", "blanket impls", "dyn clone"],
    """
        `Shape: Clone` makes `dyn Shape` impossible (E0038), and `scale_all` calls a `scaled_box` that doesn't
        exist. Make both functions work:

        - `Vec<Box<dyn Shape>>` must be cloneable, so `snapshot` compiles as written.
        - `scaled_box(&self, k) -> Box<dyn Shape>` must work on `dyn Shape`, and `scaled` must keep returning the
          concrete type.
        - Shapes defined elsewhere (the tests define one) derive `Clone` and implement only `area`, `name` and
          `scaled`.
    """,
    r"""
    pub trait Shape: Clone {
        fn area(&self) -> f64;
        fn name(&self) -> String;

        /// A copy scaled by `k` (every length times k).
        fn scaled(&self, k: f64) -> Self;
    }
    """ + CLONE_HEAD + CLONE_IMPLS + CLONE_FNS,
    CLONE_SOLUTION,
    [T("snapshot_is_independent", "snapshot, then replace scene[0]", "(snap[0].name(), scene[0].name())", '("circle(1)".to_string(), "rect(1x1)".to_string())',
       setup="let mut scene: Vec<Box<dyn Shape>> = vec![Box::new(Circle { r: 1.0 })];\nlet snap = snapshot(&scene);\nscene[0] = Box::new(Rect { w: 1.0, h: 1.0 });"),
     T("scale_all_names", "scale_all([Circle 1, Rect 2x3], 2)", "scale_all(&scene, 2.0).iter().map(|s| s.name()).collect::<Vec<_>>()", 'vec!["circle(2)", "rect(4x6)"]',
       setup="let scene: Vec<Box<dyn Shape>> = vec![Box::new(Circle { r: 1.0 }), Box::new(Rect { w: 2.0, h: 3.0 })];"),
     T("scaled_stays_concrete", "Circle { r: 1.5 }.scaled(2.0)", "Circle { r: 1.5 }.scaled(2.0)", "Circle { r: 3.0 }"),
     T("vec_clone", "scene.clone() keeps names", "scene.clone().iter().map(|s| s.name()).collect::<Vec<_>>()", 'vec!["rect(1x2)"]',
       setup="let scene: Vec<Box<dyn Shape>> = vec![Box::new(Rect { w: 1.0, h: 2.0 })];"),
     """
     #[test]
     fn a_new_shape_gets_clone_box() {
         #[derive(Clone)]
         struct Sq(f64);
         impl Shape for Sq {
             fn area(&self) -> f64 {
                 self.0 * self.0
             }
             fn name(&self) -> String {
                 format!("sq({})", self.0)
             }
             fn scaled(&self, k: f64) -> Self {
                 Sq(self.0 * k)
             }
         }
         let scene: Vec<Box<dyn Shape>> = vec![Box::new(Sq(3.0))];
         check!("scale_all([Sq(3)], 0.5)[0].area()", scale_all(&scene, 0.5)[0].area(), 2.25);
         check!("snapshot([Sq(3)])[0].name()", snapshot(&scene)[0].name(), "sq(3)");
     }
     """],
    [T("box_clone_is_a_new_allocation", "b.clone() vs b", "std::ptr::addr_eq(b.as_ref(), c.as_ref())", "false",
       setup="let b: Box<dyn Shape> = Box::new(Circle { r: 1.0 });\nlet c = b.clone();"),
     T("scaled_box_on_dyn", "Box<dyn Shape> Rect 2x2 .scaled_box(1.5)", "b.scaled_box(1.5).area()", "9.0", setup="let b: Box<dyn Shape> = Box::new(Rect { w: 2.0, h: 2.0 });"),
     T("scale_by_zero", "scale_all([Rect 3x4], 0)", "scale_all(&scene, 0.0)[0].area()", "0.0", setup="let scene: Vec<Box<dyn Shape>> = vec![Box::new(Rect { w: 3.0, h: 4.0 })];"),
     T("scale_leaves_input", "scale_all then read the input", "(scale_all(&scene, 3.0)[0].name(), scene[0].name())", '("circle(6)".to_string(), "circle(2)".to_string())',
       setup="let scene: Vec<Box<dyn Shape>> = vec![Box::new(Circle { r: 2.0 })];"),
     T("empty_scene", "snapshot(&vec![]), scale_all(&[], 2)", "(snapshot(&vec![]).len(), scale_all(&[], 2.0).len())", "(0, 0)"),
     T("rect_scaled", "Rect { w: 1, h: 4 }.scaled(0.5)", "Rect { w: 1.0, h: 4.0 }.scaled(0.5)", "Rect { w: 0.5, h: 2.0 }"),
     T("clone_of_clone", "b.clone().clone()", "b.clone().clone().name()", '"circle(7)"', setup="let b: Box<dyn Shape> = Box::new(Circle { r: 7.0 });"),
     T("circle_area_scales_by_k_squared", "Circle 1 scaled_box(3)", 'format!("{:.4}", Circle { r: 1.0 }.scaled_box(3.0).area())', '"28.2743"'),
     T("snapshot_many", "snapshot of 3 shapes", "snapshot(&scene).iter().map(|s| s.name()).collect::<Vec<_>>()", 'vec!["circle(1)", "rect(2x2)", "circle(3)"]',
       setup="let scene: Vec<Box<dyn Shape>> = vec![Box::new(Circle { r: 1.0 }), Box::new(Rect { w: 2.0, h: 2.0 }), Box::new(Circle { r: 3.0 })];"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4413);
         for _ in 0..200 {
             let n = rng.below(5);
             let dims: Vec<(f64, f64)> = (0..n).map(|_| (rng.int(1, 4) as f64, rng.int(1, 4) as f64)).collect();
             let scene: Vec<Box<dyn Shape>> = dims.iter().map(|&(w, h)| Box::new(Rect { w, h }) as Box<dyn Shape>).collect();
             let k = rng.int(0, 4) as f64 / 2.0;
             let got: Vec<f64> = scale_all(&scene, k).iter().map(|s| s.area()).collect();
             let want: Vec<f64> = dims.iter().map(|&(w, h)| w * k * h * k).collect();
             check!(format!("scale_all({dims:?}, {k})"), got, want);
         }
     }
     """],
    [("approach", "Split the trait: the object-safe methods that return `Box<dyn Shape>` (`clone_box`, `scaled_box`) go in a helper supertrait with one blanket impl for every `T: Shape + Clone + 'static`. Then `impl Clone for Box<dyn Shape>` calls `clone_box`."),
     ("rust", "`scaled` returns `Self`, so it needs `where Self: Sized`. Implementors may leave that clause off in their impls."),
     ("edge case", "In `impl Clone for Box<dyn Shape>`, call `(**self).clone_box()`: on the Box itself, method lookup could pick `Box`'s own `clone` and recurse.")],
    ("""`Clone` returns `Self`, so it can't be called on `dyn Shape`, and a supertrait `Clone` makes the whole trait dyn-incompatible. The standard fix (what the `dyn-clone` crate does) is a helper trait with a `clone_box(&self) -> Box<dyn Shape>` method, implemented once by a blanket impl for every clonable shape, plus `impl Clone for Box<dyn Shape>`. Methods that return `Self` stay available on concrete types with `where Self: Sized`.

Syntax: `trait Shape: DynShape { fn scaled(&self, k: f64) -> Self where Self: Sized; }` · `impl<T: Shape + Clone + 'static> DynShape for T { fn clone_box(&self) -> Box<dyn Shape> { Box::new(self.clone()) } }` · `impl Clone for Box<dyn Shape> { fn clone(&self) -> Self { (**self).clone_box() } }`.""", "O(n) to clone a scene", "O(n)"),
    "Why does the blanket impl need `'static`? What would you change to clone `Box<dyn Shape + 'a>`?",
    ["A supertrait that returns `Self` (like `Clone`) blocks `dyn`.", "`clone_box` via a blanket impl + `impl Clone for Box<dyn Trait>`.", "`where Self: Sized` keeps `Self`-returning methods for concrete types."],
    related=("L5", "S8"),
    wrong=dict(
        rect_scales_width_only=sub(CLONE_SOLUTION, "Rect { w: self.w * k, h: self.h * k }", "Rect { w: self.w * k, h: self.h }"),
        circle_scales_by_k_squared=sub(CLONE_SOLUTION, "Circle { r: self.r * k }", "Circle { r: self.r * k * k }"),
    ),
))

# ---------------------------------------------------------------- operator traits (medium)

VEC2_HEAD = r"""
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vec2 {
    pub x: i64,
    pub y: i64,
}
"""

VEC2_IMPLS = r"""
impl Add for Vec2 {
    type Output = Vec2;

    fn add(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x + o.x, y: self.y + o.y }
    }
}

impl Sub for Vec2 {
    type Output = Vec2;

    fn sub(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x - o.x, y: self.y - o.y }
    }
}

impl Neg for Vec2 {
    type Output = Vec2;

    fn neg(self) -> Vec2 {
        Vec2 { x: -self.x, y: -self.y }
    }
}

impl AddAssign for Vec2 {
    fn add_assign(&mut self, o: Vec2) {
        self.x += o.x;
        self.y += o.y;
    }
}

/// Scaling: v * k.
impl Mul<i64> for Vec2 {
    type Output = Vec2;

    fn mul(self, k: i64) -> Vec2 {
        Vec2 { x: self.x * k, y: self.y * k }
    }
}

/// Scaling: k * v. Allowed although i64 is foreign, because Vec2 is local.
impl Mul<Vec2> for i64 {
    type Output = Vec2;

    fn mul(self, v: Vec2) -> Vec2 {
        v * self
    }
}

/// Dot product.
impl Mul for Vec2 {
    type Output = i64;

    fn mul(self, o: Vec2) -> i64 {
        self.x * o.x + self.y * o.y
    }
}

impl Sum for Vec2 {
    fn sum<I: Iterator<Item = Vec2>>(iter: I) -> Vec2 {
        iter.fold(Vec2::default(), |a, b| a + b)
    }
}

impl<'a> Sum<&'a Vec2> for Vec2 {
    fn sum<I: Iterator<Item = &'a Vec2>>(iter: I) -> Vec2 {
        iter.copied().sum()
    }
}
"""

VEC2_TAIL = r"""
/// The sum of all the points.
pub fn total(points: &[Vec2]) -> Vec2 {
    points.iter().sum()
}

/// Start at the origin and take each step `count` times.
pub fn walk(steps: &[(Vec2, i64)]) -> Vec2 {
    let mut p = Vec2::default();
    for &(d, count) in steps {
        p += d * count;
    }
    p
}
"""

VEC2_SOLUTION = VEC2_HEAD + VEC2_IMPLS + VEC2_TAIL

P.append(fix(
    "vec2-operators", "Operator overloading: Add, Mul, Neg and Sum", "medium", "operator-traits", ["Add", "Mul", "Neg", "AddAssign", "Sum", "operator traits"],
    """
        Give `Vec2` its operators; `total`, `walk` and the tests use them.

        - `a + b`, `a - b`, `-a`, and `a += b`.
        - Scaling by an `i64` from either side: `v * 3` and `3 * v`.
        - `a * b` is the dot product, an `i64`.
        - Summing an iterator of `Vec2` or of `&Vec2` with `.sum()`.
    """,
    VEC2_HEAD + "\n// TODO: the operator impls.\n" + VEC2_TAIL,
    VEC2_SOLUTION,
    [T("add_sub_neg", "a = (1, 2), b = (10, 20): a + b, b - a, -a", "(a + b, b - a, -a)", "(Vec2 { x: 11, y: 22 }, Vec2 { x: 9, y: 18 }, Vec2 { x: -1, y: -2 })",
       setup="let (a, b) = (Vec2 { x: 1, y: 2 }, Vec2 { x: 10, y: 20 });"),
     T("scale_both_sides", "v = (2, -3): v * 3, 3 * v", "(v * 3, 3 * v)", "(Vec2 { x: 6, y: -9 }, Vec2 { x: 6, y: -9 })", setup="let v = Vec2 { x: 2, y: -3 };"),
     T("dot_product", "(1, 2) * (3, 4)", "Vec2 { x: 1, y: 2 } * Vec2 { x: 3, y: 4 }", "11"),
     T("walk_steps", "walk [((1, 0), 3), ((0, 1), 2)]", "walk(&[(Vec2 { x: 1, y: 0 }, 3), (Vec2 { x: 0, y: 1 }, 2)])", "Vec2 { x: 3, y: 2 }"),
     T("sum_owned_and_borrowed", "sum of [(1, 1), (2, 3)], owned and by reference", "(v.iter().sum::<Vec2>(), v.into_iter().sum::<Vec2>())", "(Vec2 { x: 3, y: 4 }, Vec2 { x: 3, y: 4 })",
       setup="let v = vec![Vec2 { x: 1, y: 1 }, Vec2 { x: 2, y: 3 }];")],
    [T("total_empty", "total(&[])", "total(&[])", "Vec2 { x: 0, y: 0 }"),
     T("add_assign", "p += (5, -5) twice", "p", "Vec2 { x: 11, y: -9 }", setup="let mut p = Vec2 { x: 1, y: 1 };\np += Vec2 { x: 5, y: -5 };\np += Vec2 { x: 5, y: -5 };"),
     T("perpendicular_dot", "(3, 4) * (-4, 3)", "Vec2 { x: 3, y: 4 } * Vec2 { x: -4, y: 3 }", "0"),
     T("scale_by_zero", "0 * (7, 8)", "0 * Vec2 { x: 7, y: 8 }", "Vec2 { x: 0, y: 0 }"),
     T("scale_by_negative", "(7, -8) * -2", "Vec2 { x: 7, y: -8 } * -2", "Vec2 { x: -14, y: 16 }"),
     T("double_negation", "-(-(5, 6))", "-(-Vec2 { x: 5, y: 6 })", "Vec2 { x: 5, y: 6 }"),
     T("walk_backwards", "walk [((2, 1), -3)]", "walk(&[(Vec2 { x: 2, y: 1 }, -3)])", "Vec2 { x: -6, y: -3 }"),
     T("sub_is_not_commutative", "(0, 0) - (1, 2)", "Vec2 { x: 0, y: 0 } - Vec2 { x: 1, y: 2 }", "Vec2 { x: -1, y: -2 }"),
     T("sum_of_mapped", "(1..=4) mapped to (i, i*i), summed", "(1..=4).map(|i| Vec2 { x: i, y: i * i }).sum::<Vec2>()", "Vec2 { x: 10, y: 30 }"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4414);
         for _ in 0..300 {
             let c: Vec<i64> = rng.vec(4, -50, 50);
             let k = rng.int(-9, 9);
             let (a, b) = (Vec2 { x: c[0], y: c[1] }, Vec2 { x: c[2], y: c[3] });
             let d = format!("a = {a:?}, b = {b:?}, k = {k}");
             check!(d.clone(), a + b * k, Vec2 { x: c[0] + c[2] * k, y: c[1] + c[3] * k });
             check!(d.clone(), k * (a - b), Vec2 { x: k * (c[0] - c[2]), y: k * (c[1] - c[3]) });
             check!(d.clone(), a * b, c[0] * c[2] + c[1] * c[3]);
             check!(d.clone(), -a + b, Vec2 { x: c[2] - c[0], y: c[3] - c[1] });
             let n = rng.below(5);
             let pts: Vec<Vec2> = (0..n).map(|_| Vec2 { x: rng.int(-9, 9), y: rng.int(-9, 9) }).collect();
             let want = pts.iter().fold((0, 0), |(x, y), p| (x + p.x, y + p.y));
             check!(format!("total({pts:?})"), total(&pts), Vec2 { x: want.0, y: want.1 });
         }
     }
     """],
    [("rust", "Each binary operator is a trait with a right-hand type and an `Output`: `impl Mul<i64> for Vec2 { type Output = Vec2; .. }`, and the dot product is `impl Mul for Vec2 { type Output = i64; .. }`. For `3 * v`, implement on the left type: `impl Mul<Vec2> for i64`."),
     ("rust", "`.sum()` needs `impl Sum for Vec2` for owned items and `impl<'a> Sum<&'a Vec2> for Vec2` for borrowed ones.")],
    ("""Operators are traits in `std::ops`; the right-hand side is a type parameter (default `Self`) and the result an associated type, so `Vec2 * i64` and `Vec2 * Vec2` can return different things. `impl Mul<Vec2> for i64` is allowed by coherence because a local type appears in it. `+=` is a separate trait (`AddAssign`), and `Iterator::sum` goes through `Sum<A>`.

Syntax: `impl Mul<i64> for Vec2 { type Output = Vec2; fn mul(self, k: i64) -> Vec2 { .. } }` · `impl Neg for Vec2 { type Output = Vec2; fn neg(self) -> Vec2 }` · `impl AddAssign for Vec2 { fn add_assign(&mut self, o: Vec2) }` · `impl<'a> Sum<&'a Vec2> for Vec2 { fn sum<I: Iterator<Item = &'a Vec2>>(iter: I) -> Vec2 }`.""", "O(1) per operation", "O(1)"),
    "Would you also implement `Mul<Vec2> for Vec2` as the dot product in a real library? What does `nalgebra` do instead?",
    ["Operator traits have an `Rhs` parameter and an `Output` type.", "Foreign-left impls like `impl Mul<Vec2> for i64` are fine when a local type appears.", "`Sum` for owned and `&'a` items."],
    related=("S6", "S8"),
    wrong=dict(
        dot_is_cross=sub(VEC2_SOLUTION, "self.x * o.x + self.y * o.y", "self.x * o.y - self.y * o.x"),
        left_scale_x_only=sub(VEC2_SOLUTION, "    fn mul(self, v: Vec2) -> Vec2 {\n        v * self\n    }", "    fn mul(self, v: Vec2) -> Vec2 {\n        Vec2 { x: v.x * self, y: v.y }\n    }"),
        sub_reversed=sub(VEC2_SOLUTION, "Vec2 { x: self.x - o.x, y: self.y - o.y }", "Vec2 { x: o.x - self.x, y: o.y - self.y }"),
    ),
))

POLY_STARTER = r"""
use std::ops::{Add, AddAssign};

/// Coefficients from the constant term up: Poly(vec![1, 0, 3]) is 1 + 3x².
/// No trailing zeros: the zero polynomial is Poly(vec![]).
#[derive(Debug, PartialEq, Eq)]
pub struct Poly(pub Vec<i64>);

impl Add for Poly {
    type Output = Poly;

    fn add(self, rhs: Poly) -> Poly {
        let (mut long, short) = if self.0.len() >= rhs.0.len() { (self.0, rhs.0) } else { (rhs.0, self.0) };
        for (i, c) in short.into_iter().enumerate() {
            long[i] += c;
        }
        while long.last() == Some(&0) {
            long.pop();
        }
        Poly(long)
    }
}

/// The sum of all the polynomials.
pub fn sum_all(ps: &[Poly]) -> Poly {
    ps.iter().fold(Poly(vec![]), |acc, p| acc + p)
}
"""

POLY_ADDED = r"""
impl AddAssign<&Poly> for Poly {
    fn add_assign(&mut self, rhs: &Poly) {
        if self.0.len() < rhs.0.len() {
            self.0.resize(rhs.0.len(), 0);
        }
        for (a, b) in self.0.iter_mut().zip(&rhs.0) {
            *a += b;
        }
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }
}

/// Reuses the left side's buffer.
impl Add<&Poly> for Poly {
    type Output = Poly;

    fn add(mut self, rhs: &Poly) -> Poly {
        self += rhs;
        self
    }
}

impl Add<&Poly> for &Poly {
    type Output = Poly;

    fn add(self, rhs: &Poly) -> Poly {
        Poly(Vec::with_capacity(self.0.len().max(rhs.0.len()))) + self + rhs
    }
}
"""

POLY_SOLUTION = sub(POLY_STARTER, "\n/// The sum of all the polynomials.", POLY_ADDED + "\n/// The sum of all the polynomials.")

P.append(fix(
    "fix-add-for-references", "Fix: adding through references (E0308)", "medium", "operator-traits", ["E0308", "Add<&T>", "AddAssign", "operator traits"],
    """
        `sum_all` doesn't compile: `Poly` only adds by value, and `p` is a `&Poly`. Make these work without cloning
        a polynomial:

        - `poly + &other`, reusing `poly`'s buffer;
        - `&a + &b`, leaving both usable;
        - `poly += &other`.

        Results never end in zeros.
    """,
    POLY_STARTER,
    POLY_SOLUTION,
    [T("sum_all_three", "sum_all([1 + 2x, 3, x²])", "sum_all(&[Poly(vec![1, 2]), Poly(vec![3]), Poly(vec![0, 0, 1])])", "Poly(vec![4, 2, 1])"),
     T("owned_plus_ref", "Poly [1, 2] + &Poly [0, 0, 5]", "Poly(vec![1, 2]) + &Poly(vec![0, 0, 5])", "Poly(vec![1, 2, 5])"),
     T("ref_plus_ref_keeps_both", "&a + &b, then a and b", "(&a + &b, a, b)", "(Poly(vec![3, 3]), Poly(vec![1, 2]), Poly(vec![2, 1]))",
       setup="let a = Poly(vec![1, 2]);\nlet b = Poly(vec![2, 1]);"),
     T("add_assign_ref", "p = [5]; p += &[1, 1]", "p", "Poly(vec![6, 1])", setup="let mut p = Poly(vec![5]);\np += &Poly(vec![1, 1]);"),
     T("cancelling_trims", "[1, 2] + &[0, -2]", "Poly(vec![1, 2]) + &Poly(vec![0, -2])", "Poly(vec![1])")],
    [T("sum_all_empty", "sum_all(&[])", "sum_all(&[])", "Poly(vec![])"),
     T("all_cancel", "&[1, -1] + &[-1, 1]", "&Poly(vec![1, -1]) + &Poly(vec![-1, 1])", "Poly(vec![])"),
     T("zero_plus_zero", "Poly [] + &Poly []", "Poly(vec![]) + &Poly(vec![])", "Poly(vec![])"),
     T("longer_right", "[1] + &[0, 0, 0, 4]", "Poly(vec![1]) + &Poly(vec![0, 0, 0, 4])", "Poly(vec![1, 0, 0, 4])"),
     T("add_assign_trims", "p = [1, 2, 3]; p += &[0, 0, -3]", "p", "Poly(vec![1, 2])", setup="let mut p = Poly(vec![1, 2, 3]);\np += &Poly(vec![0, 0, -3]);"),
     T("add_assign_self_copy", "p += &q twice", "p", "Poly(vec![2, 4])", setup="let mut p = Poly(vec![]);\nlet q = Poly(vec![1, 2]);\np += &q;\np += &q;"),
     T("owned_by_value_still_works", "Poly [1] + Poly [2]", "Poly(vec![1]) + Poly(vec![2])", "Poly(vec![3])"),
     T("chained", "&a + &b + &c", "&a + &b + &c", "Poly(vec![1, 1, 1])", setup="let (a, b, c) = (Poly(vec![1]), Poly(vec![0, 1]), Poly(vec![0, 0, 1]));"),
     T("sum_all_many", "sum_all of 1000 copies of [1, -1, 2]", "sum_all(&ps)", "Poly(vec![1000, -1000, 2000])", setup="let ps: Vec<Poly> = (0..1000).map(|_| Poly(vec![1, -1, 2])).collect();"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4415);
         for _ in 0..300 {
             let n = rng.below(5);
             let mut ps = Vec::new();
             for _ in 0..n {
                 let len = rng.below(4);
                 let mut c: Vec<i64> = rng.vec(len, -2, 2);
                 while c.last() == Some(&0) {
                     c.pop();
                 }
                 ps.push(Poly(c));
             }
             let width = ps.iter().map(|p| p.0.len()).max().unwrap_or(0);
             let mut want = vec![0i64; width];
             for p in &ps {
                 for (i, c) in p.0.iter().enumerate() {
                     want[i] += c;
                 }
             }
             while want.last() == Some(&0) {
                 want.pop();
             }
             check!(format!("sum_all({ps:?})"), sum_all(&ps), Poly(want.clone()));
             if n >= 2 {
                 let mut acc = Poly(vec![]);
                 for p in &ps {
                     acc += p;
                 }
                 check!(format!("+= over {ps:?}"), acc, Poly(want.clone()));
                 let pair = &ps[0] + &ps[1];
                 let mut w2 = vec![0i64; ps[0].0.len().max(ps[1].0.len())];
                 for p in &ps[..2] {
                     for (i, c) in p.0.iter().enumerate() {
                         w2[i] += c;
                     }
                 }
                 while w2.last() == Some(&0) {
                     w2.pop();
                 }
                 check!(format!("{:?} + {:?}", ps[0], ps[1]), pair, Poly(w2));
             }
         }
     }
     """],
    [("rust", "`Add` has a type parameter for the right side: implement `Add<&Poly> for Poly` and `Add<&Poly> for &Poly`, each with `type Output = Poly`. `AddAssign<&Poly> for Poly` gives `+=`."),
     ("approach", "Write the in-place addition once, in `add_assign`, and build the other two on it: `Poly + &Poly` is `self += rhs; self`."),
     ("edge case", "Pad the left side when the right one is longer, and pop trailing zeros after adding.")],
    ("""`a + b` desugars to `Add::add(a, b)`, which takes both sides by value. For non-`Copy` types that means implementing the trait again for references (`impl Add<&Poly> for &Poly`), as `String + &str` and `BigInt` do. Taking the left side by value lets `+` reuse its buffer, so a fold does no reallocation beyond growth.

Syntax: `impl Add<&Poly> for Poly { type Output = Poly; fn add(mut self, rhs: &Poly) -> Poly { self += rhs; self } }` · `impl AddAssign<&Poly> for Poly { fn add_assign(&mut self, rhs: &Poly) }`.""", "O(len) per addition", "O(len)"),
    "`String` implements `Add<&str>` but not `Add<String>` or `&String + &str`. Why that choice?",
    ["Operators take operands by value; add impls for `&T` to add through references.", "Write the mutating version once and derive the others from it."],
    rules=dict(methods=["clone", "cloned", "to_vec", "to_owned"]),
    related=("S3", "S8"),
    wrong=dict(
        no_trim=sub(POLY_SOLUTION, "            *a += b;\n        }\n        while self.0.last() == Some(&0) {\n            self.0.pop();\n        }", "            *a += b;\n        }"),
        drops_longer_tail=sub(POLY_SOLUTION, "        if self.0.len() < rhs.0.len() {\n            self.0.resize(rhs.0.len(), 0);\n        }\n", ""),
    ),
))

MATRIX_SOLUTION = r"""
use std::ops::{Add, Index, IndexMut, Mul};

/// A rows × cols matrix, stored row by row in one Vec.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T> {
    rows: usize,
    cols: usize,
    data: Vec<T>,
}

impl<T: Copy + Default> Matrix<T> {
    /// Every entry `T::default()`.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Matrix { rows, cols, data: vec![T::default(); rows * cols] }
    }

    /// Panics if the rows have different lengths. No rows gives a 0 × 0 matrix.
    pub fn from_rows(rows: Vec<Vec<T>>) -> Self {
        let r = rows.len();
        let c = rows.first().map_or(0, Vec::len);
        assert!(rows.iter().all(|row| row.len() == c), "rows have different lengths");
        Matrix { rows: r, cols: c, data: rows.into_iter().flatten().collect() }
    }

    pub fn transpose(&self) -> Self {
        let mut t = Matrix::zeros(self.cols, self.rows);
        for r in 0..self.rows {
            for c in 0..self.cols {
                t[(c, r)] = self[(r, c)];
            }
        }
        t
    }
}

impl<T> Matrix<T> {
    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Row `r` as a slice. Panics if `r` is out of range.
    pub fn row(&self, r: usize) -> &[T] {
        assert!(r < self.rows, "row {r} out of range");
        &self.data[r * self.cols..(r + 1) * self.cols]
    }

    /// Every row, top to bottom.
    pub fn rows_iter(&self) -> impl Iterator<Item = &[T]> {
        (0..self.rows).map(move |r| self.row(r))
    }
}

/// m[(r, c)]. Panics if `r` or `c` is out of range.
impl<T> Index<(usize, usize)> for Matrix<T> {
    type Output = T;

    fn index(&self, (r, c): (usize, usize)) -> &T {
        assert!(r < self.rows && c < self.cols, "({r}, {c}) is outside {}x{}", self.rows, self.cols);
        &self.data[r * self.cols + c]
    }
}

impl<T> IndexMut<(usize, usize)> for Matrix<T> {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut T {
        assert!(r < self.rows && c < self.cols, "({r}, {c}) is outside {}x{}", self.rows, self.cols);
        &mut self.data[r * self.cols + c]
    }
}

/// Element-wise sum. Panics if the shapes differ.
impl<T: Copy + Add<Output = T>> Add for Matrix<T> {
    type Output = Matrix<T>;

    fn add(mut self, rhs: Matrix<T>) -> Matrix<T> {
        assert!(self.rows == rhs.rows && self.cols == rhs.cols, "shapes differ");
        for (a, &b) in self.data.iter_mut().zip(&rhs.data) {
            *a = *a + b;
        }
        self
    }
}

/// Matrix product. Panics unless `self.cols() == rhs.rows()`.
impl<'a, T: Copy + Default + Add<Output = T> + Mul<Output = T>> Mul for &'a Matrix<T> {
    type Output = Matrix<T>;

    fn mul(self, rhs: &'a Matrix<T>) -> Matrix<T> {
        assert_eq!(self.cols, rhs.rows, "inner dimensions differ");
        let mut out = Matrix::zeros(self.rows, rhs.cols);
        for i in 0..self.rows {
            for k in 0..self.cols {
                let a = self[(i, k)];
                for j in 0..rhs.cols {
                    out[(i, j)] = out[(i, j)] + a * rhs[(k, j)];
                }
            }
        }
        out
    }
}
"""

MATRIX_STARTER = r"""
use std::ops::{Add, Index, IndexMut, Mul};

/// A rows × cols matrix, stored row by row in one Vec.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T> {
    rows: usize,
    cols: usize,
    data: Vec<T>,
}

impl<T: Copy + Default> Matrix<T> {
    /// Every entry `T::default()`.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        todo!()
    }

    /// Panics if the rows have different lengths. No rows gives a 0 × 0 matrix.
    pub fn from_rows(rows: Vec<Vec<T>>) -> Self {
        todo!()
    }

    pub fn transpose(&self) -> Self {
        todo!()
    }
}

impl<T> Matrix<T> {
    pub fn rows(&self) -> usize {
        todo!()
    }

    pub fn cols(&self) -> usize {
        todo!()
    }

    /// Row `r` as a slice. Panics if `r` is out of range.
    pub fn row(&self, r: usize) -> &[T] {
        todo!()
    }

    /// Every row, top to bottom.
    pub fn rows_iter(&self) -> impl Iterator<Item = &[T]> {
        // TODO: replace the placeholder.
        std::iter::from_fn(|| todo!())
    }
}

/// m[(r, c)]. Panics if `r` or `c` is out of range.
impl<T> Index<(usize, usize)> for Matrix<T> {
    type Output = T;

    fn index(&self, (r, c): (usize, usize)) -> &T {
        todo!()
    }
}

impl<T> IndexMut<(usize, usize)> for Matrix<T> {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut T {
        todo!()
    }
}

/// Element-wise sum. Panics if the shapes differ.
impl<T: Copy + Add<Output = T>> Add for Matrix<T> {
    type Output = Matrix<T>;

    fn add(self, rhs: Matrix<T>) -> Matrix<T> {
        todo!()
    }
}

/// Matrix product. Panics unless `self.cols() == rhs.rows()`.
impl<'a, T: Copy + Default + Add<Output = T> + Mul<Output = T>> Mul for &'a Matrix<T> {
    type Output = Matrix<T>;

    fn mul(self, rhs: &'a Matrix<T>) -> Matrix<T> {
        todo!()
    }
}
"""

MATRIX_HELP = r"""
fn m(rows: &[&[i64]]) -> Matrix<i64> {
    Matrix::from_rows(rows.iter().map(|r| r.to_vec()).collect())
}

fn panics<R>(f: impl FnOnce() -> R) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
}
"""

P.append(write(
    "matrix-operators", "Matrix<T> with operator overloading", "medium", "operator-traits", ["Index", "IndexMut", "Add", "Mul", "generic bounds"],
    """
        Implement a generic `Matrix<T>` stored in one `Vec<T>`, row by row. The bounds on each `impl` are already
        chosen; notice which operations need which.

        - `zeros`, `from_rows` (panics on ragged rows), `transpose`, `rows`, `cols`, `row(r)` and `rows_iter()`.
        - `m[(r, c)]` reads and writes an entry, and panics if either index is out of range.
        - `a + b` adds element-wise; `&a * &b` is the matrix product. Both panic on mismatched shapes.
    """,
    MATRIX_STARTER,
    MATRIX_SOLUTION,
    [MATRIX_HELP,
     T("add", "[[1,2],[3,4]] + [[5,6],[7,8]]", "m(&[&[1, 2], &[3, 4]]) + m(&[&[5, 6], &[7, 8]])", "m(&[&[6, 8], &[10, 12]])"),
     T("product", "&[[1,2],[3,4]] * &[[5,6],[7,8]]", "&m(&[&[1, 2], &[3, 4]]) * &m(&[&[5, 6], &[7, 8]])", "m(&[&[19, 22], &[43, 50]])"),
     T("transpose_non_square", "transpose [[1,2,3],[4,5,6]]", "m(&[&[1, 2, 3], &[4, 5, 6]]).transpose()", "m(&[&[1, 4], &[2, 5], &[3, 6]])"),
     T("index_mut_then_row", "a[(1, 0)] = 9; a.row(1)", "(a.row(1).to_vec(), a[(0, 1)])", "(vec![9, 4], 2)", setup="let mut a = m(&[&[1, 2], &[3, 4]]);\na[(1, 0)] = 9;"),
     T("rows_iter_sums", "row sums of [[1,2],[3,4]]", "m(&[&[1, 2], &[3, 4]]).rows_iter().map(|r| r.iter().sum::<i64>()).collect::<Vec<_>>()", "vec![3, 7]"),
     T("column_out_of_range_panics", "a[(0, 2)] on a 2×2", "panics(|| a[(0, 2)])", "true", setup="let a = m(&[&[1, 2], &[3, 4]]);")],
    [MATRIX_HELP,
     T("row_out_of_range_panics", "a[(2, 0)] on a 2×2", "panics(|| a[(2, 0)])", "true", setup="let a = m(&[&[1, 2], &[3, 4]]);"),
     T("mul_shape_mismatch_panics", "2×2 times 3×1", "panics(|| &a * &b)", "true", setup="let a = m(&[&[1, 2], &[3, 4]]);\nlet b = m(&[&[1], &[2], &[3]]);"),
     T("add_shape_mismatch_panics", "2×1 + 1×2", "panics(|| m(&[&[1], &[2]]) + m(&[&[1, 2]]))", "true"),
     T("ragged_panics", "from_rows [[1, 2], [3]]", "panics(|| m(&[&[1, 2], &[3]]))", "true"),
     T("non_square_product", "2×3 times 3×1", "&m(&[&[1, 2, 3], &[4, 5, 6]]) * &m(&[&[1], &[0], &[-1]])", "m(&[&[-2], &[-2]])"),
     T("empty", "from_rows(vec![])", "(e.rows(), e.cols(), e.rows_iter().count())", "(0, 0, 0)", setup="let e: Matrix<i64> = Matrix::from_rows(vec![]);"),
     T("zero_width_rows", "zeros(3, 0).rows_iter()", "z.rows_iter().map(|r| r.len()).collect::<Vec<_>>()", "vec![0, 0, 0]", setup="let z: Matrix<i64> = Matrix::zeros(3, 0);"),
     T("inner_dimension_zero", "(2×0) * (0×3)", "&Matrix::<i64>::zeros(2, 0) * &Matrix::zeros(0, 3)", "Matrix::zeros(2, 3)"),
     T("floats", "&[[0.5, 1.5]] * &[[2.0], [4.0]]", "&Matrix::from_rows(vec![vec![0.5, 1.5]]) * &Matrix::from_rows(vec![vec![2.0], vec![4.0]])", "Matrix::from_rows(vec![vec![7.0]])"),
     T("transpose_twice", "transpose of transpose", "a.transpose().transpose() == a", "true", setup="let a = m(&[&[1, 2, 3], &[4, 5, 6]]);"),
     """
     #[test]
     fn any_ring_type() {
         // T only needs Copy + Default + Add + Mul.
         #[derive(Debug, Clone, Copy, PartialEq, Default)]
         struct Mod7(u8);
         impl std::ops::Add for Mod7 {
             type Output = Mod7;
             fn add(self, o: Mod7) -> Mod7 {
                 Mod7((self.0 + o.0) % 7)
             }
         }
         impl std::ops::Mul for Mod7 {
             type Output = Mod7;
             fn mul(self, o: Mod7) -> Mod7 {
                 Mod7((self.0 * o.0) % 7)
             }
         }
         let a = Matrix::from_rows(vec![vec![Mod7(3), Mod7(4)], vec![Mod7(5), Mod7(6)]]);
         let b = Matrix::from_rows(vec![vec![Mod7(2), Mod7(0)], vec![Mod7(1), Mod7(3)]]);
         check!("[[3,4],[5,6]] * [[2,0],[1,3]] mod 7", &a * &b, Matrix::from_rows(vec![vec![Mod7(3), Mod7(5)], vec![Mod7(2), Mod7(4)]]));
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4416);
         for _ in 0..200 {
             let (r, k, c) = (rng.below(4) + 1, rng.below(4) + 1, rng.below(4) + 1);
             let a: Vec<Vec<i64>> = (0..r).map(|_| rng.vec(k, -5, 5)).collect();
             let b: Vec<Vec<i64>> = (0..k).map(|_| rng.vec(c, -5, 5)).collect();
             let mut want = vec![vec![0i64; c]; r];
             for i in 0..r {
                 for j in 0..c {
                     for x in 0..k {
                         want[i][j] += a[i][x] * b[x][j];
                     }
                 }
             }
             let (ma, mb) = (Matrix::from_rows(a.clone()), Matrix::from_rows(b.clone()));
             check!(format!("{a:?} * {b:?}"), &ma * &mb, Matrix::from_rows(want));
             let at: Vec<Vec<i64>> = (0..k).map(|j| (0..r).map(|i| a[i][j]).collect()).collect();
             check!(format!("transpose {a:?}"), ma.transpose(), Matrix::from_rows(at));
             let doubled: Vec<Vec<i64>> = a.iter().map(|row| row.iter().map(|x| 2 * x).collect()).collect();
             check!(format!("{a:?} + itself"), ma.clone() + ma.clone(), Matrix::from_rows(doubled));
             let (i, j) = (rng.below(r), rng.below(k));
             check!(format!("{a:?}[({i}, {j})]"), ma[(i, j)], a[i][j]);
         }
     }
     """,
     """
     #[test]
     fn scale_200x200() {
         let n = 200;
         let a: Matrix<i64> = Matrix::from_rows((0..n).map(|i| (0..n).map(|j| ((i * 7 + j * 3) % 11) as i64).collect()).collect());
         let mut id = Matrix::zeros(n, n);
         for i in 0..n {
             id[(i, i)] = 1;
         }
         check!("A * I == A for 200 × 200", &a * &id == a, true);
         let ones: Matrix<i64> = Matrix::from_rows(vec![vec![1; n]; n]);
         let sq = &ones * &ones;
         check!("ones * ones, every entry", sq.rows_iter().all(|r| r.iter().all(|&x| x == n as i64)), true);
     }
     """],
    [("rust", "Store `data[r * cols + c]`. Check `c < cols` yourself: `(0, cols)` lands inside the Vec (on the next row), so the Vec's own bounds check won't catch it."),
     ("rust", "`Mul for &'a Matrix<T>` has `Rhs = &'a Matrix<T>`, so `&a * &b` borrows both. `rows_iter` can map row numbers to `self.row(r)`; `chunks(cols)` panics when `cols == 0`."),
     ("approach", "Loop `i, k, j` in the product so the inner loop walks rows of both `rhs` and `out`.")],
    ("""`Index`/`IndexMut` let a type choose its key (`(usize, usize)` here) and return a reference into itself. `IndexMut` requires `Index` and reuses its `Output`. The trait bounds say what each operation needs: `zeros` needs `Default`, `transpose` and the product need `Copy`, the product needs `Add` and `Mul` on `T`. Implementing `Mul` for `&Matrix` avoids consuming large operands.

Syntax: `impl<T> Index<(usize, usize)> for Matrix<T> { type Output = T; fn index(&self, (r, c): (usize, usize)) -> &T }` · `impl<T> IndexMut<(usize, usize)> for Matrix<T> { fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut T }` · `impl<'a, T: ..> Mul for &'a Matrix<T> { type Output = Matrix<T>; .. }`.""", "O(r·k·c) product; O(r·c) add and transpose", "O(r·c)"),
    "How would you make `a + b` work for `&Matrix` too without writing the loop twice? What about `Matrix<BigInt>`, which isn't `Copy`?",
    ["`Index<Idx>` picks the key type; `IndexMut` reuses `Output`.", "Check every index dimension yourself in flat storage.", "Put only the bounds each operation needs on its `impl`."],
    related=("S3", "D13"),
    wrong=dict(
        elementwise_product=sub(MATRIX_SOLUTION, "        assert_eq!(self.cols, rhs.rows, \"inner dimensions differ\");\n        let mut out = Matrix::zeros(self.rows, rhs.cols);\n        for i in 0..self.rows {\n            for k in 0..self.cols {\n                let a = self[(i, k)];\n                for j in 0..rhs.cols {\n                    out[(i, j)] = out[(i, j)] + a * rhs[(k, j)];\n                }\n            }\n        }\n        out",
                                "        let mut out = self.clone();\n        for (a, &b) in out.data.iter_mut().zip(&rhs.data) {\n            *a = *a * b;\n        }\n        out"),
        no_column_check=sub(sub(MATRIX_SOLUTION, "        assert!(r < self.rows && c < self.cols, \"({r}, {c}) is outside {}x{}\", self.rows, self.cols);\n        &self.data[r * self.cols + c]", "        &self.data[r * self.cols + c]"),
                            "        assert!(r < self.rows && c < self.cols, \"({r}, {c}) is outside {}x{}\", self.rows, self.cols);\n        &mut self.data[r * self.cols + c]", "        &mut self.data[r * self.cols + c]"),
        rows_iter_chunks=sub(MATRIX_SOLUTION, "(0..self.rows).map(move |r| self.row(r))", "self.data.chunks(self.cols)"),
    ),
))

# ---------------------------------------------------------------- coherence & extension (hard)

PATH_HEAD = r"""
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}
"""

PATH_DISPLAY_BODY = r"""    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("(empty)");
        }
        for (i, p) in self.iter().enumerate() {
            if i > 0 {
                f.write_str(" -> ")?;
            }
            write!(f, "{p}")?;
        }
        Ok(())
    }
"""

PATH_TAIL = r"""
/// Total Manhattan length of a path.
pub fn length(path: &[Point]) -> u32 {
    path.windows(2).map(|w| w[0].x.abs_diff(w[1].x) + w[0].y.abs_diff(w[1].y)).sum()
}
"""

PATH_NEWTYPE = r"""
use std::ops::{Deref, DerefMut};

/// A path of points. A newtype, because `Display` and `Vec` are both foreign.
pub struct Path(pub Vec<Point>);

/// A path prints as its points joined by " -> ", or "(empty)".
impl fmt::Display for Path {
""" + PATH_DISPLAY_BODY + r"""}

/// Lets a Path be used like the Vec it wraps: len, indexing, iter, push, &Path as &[Point].
impl Deref for Path {
    type Target = Vec<Point>;

    fn deref(&self) -> &Vec<Point> {
        &self.0
    }
}

impl DerefMut for Path {
    fn deref_mut(&mut self) -> &mut Vec<Point> {
        &mut self.0
    }
}

impl From<Vec<Point>> for Path {
    fn from(points: Vec<Point>) -> Self {
        Path(points)
    }
}

impl FromIterator<Point> for Path {
    fn from_iter<I: IntoIterator<Item = Point>>(iter: I) -> Self {
        Path(iter.into_iter().collect())
    }
}
"""

PATH_SOLUTION = PATH_HEAD + PATH_NEWTYPE + PATH_TAIL

P.append(fix(
    "fix-orphan-rule", "Fix: the orphan rule (E0117) and newtypes", "hard", "coherence-extension", ["E0117", "orphan rule", "newtype", "Deref", "FromIterator"],
    """
        `impl Display for Vec<Point>` breaks the orphan rule (E0117). Replace it with a newtype
        `pub struct Path(pub Vec<Point>)` that prints the same way.

        Callers must still use a path like the `Vec`: `path.len()`, `path[0]`, `path.iter()`, `path.push(p)`, and
        `length(&path)`. They build one with `Path::from(vec)` or by `collect()`ing points.
    """,
    PATH_HEAD + "\n/// A path prints as its points joined by \" -> \", or \"(empty)\".\nimpl fmt::Display for Vec<Point> {\n" + PATH_DISPLAY_BODY + "}\n" + PATH_TAIL,
    PATH_SOLUTION,
    ["""
     fn p(x: i32, y: i32) -> Point {
         Point { x, y }
     }
     """,
     T("prints", "Path [(1, 2), (3, 4)]", "Path(vec![p(1, 2), p(3, 4)]).to_string()", '"(1, 2) -> (3, 4)"'),
     T("empty", "Path []", "Path(vec![]).to_string()", '"(empty)"'),
     T("slice_like", "len, [1], length(&path) of [(0, 0), (3, 4), (3, 0)]", "(path.len(), path[1], length(&path))", "(3, p(3, 4), 11)",
       setup="let path = Path::from(vec![p(0, 0), p(3, 4), p(3, 0)]);"),
     T("collect_and_push", "(0..3) mapped to (i, i) and collected, then push (9, 9)", "path.to_string()", '"(0, 0) -> (1, 1) -> (2, 2) -> (9, 9)"',
       setup="let mut path: Path = (0..3).map(|i| p(i, i)).collect();\npath.push(p(9, 9));"),
     T("iter", "sum of x over the path", "path.iter().map(|q| q.x).sum::<i32>()", "6", setup="let path = Path::from(vec![p(1, 0), p(2, 0), p(3, 0)]);")],
    ["""
     fn p(x: i32, y: i32) -> Point {
         Point { x, y }
     }
     """,
     T("single", "Path [(-1, 5)]", "Path(vec![p(-1, 5)]).to_string()", '"(-1, 5)"'),
     T("length_short_paths", "length of [] and [(1, 1)]", "(length(&Path(vec![])), length(&Path(vec![p(1, 1)])))", "(0, 0)"),
     T("length_extremes", "length of [(i32::MIN, 0), (i32::MAX, 0)]", "length(&Path(vec![p(i32::MIN, 0), p(i32::MAX, 0)]))", "u32::MAX"),
     T("format_in_sentence", "format!(\"route: {}\", path)", 'format!("route: {}", path)', '"route: (0, 0) -> (0, -1)"', setup="let path = Path(vec![p(0, 0), p(0, -1)]);"),
     T("collect_empty", "an empty iterator collected", "std::iter::empty::<Point>().collect::<Path>().to_string()", '"(empty)"'),
     T("mutate_through_index", "path[0].x = 7", "path.to_string()", '"(7, 0)"', setup="let mut path = Path(vec![p(0, 0)]);\npath[0].x = 7;"),
     T("vec_still_usable", "the inner Vec after building", "path.0.len()", "2", setup="let path = Path::from(vec![p(1, 1), p(2, 2)]);"),
     T("sort_through_deref_mut", "sort_by_key x on [(3, 0), (1, 0)]", "path.to_string()", '"(1, 0) -> (3, 0)"',
       setup="let mut path = Path(vec![p(3, 0), p(1, 0)]);\npath.sort_by_key(|q| q.x);"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4417);
         for _ in 0..300 {
             let n = rng.below(5);
             let pts: Vec<Point> = (0..n).map(|_| p(rng.int(-9, 9) as i32, rng.int(-9, 9) as i32)).collect();
             let want = if n == 0 { "(empty)".to_string() } else { pts.iter().map(|q| format!("({}, {})", q.x, q.y)).collect::<Vec<_>>().join(" -> ") };
             let path: Path = pts.iter().copied().collect();
             check!(format!("{pts:?}"), path.to_string(), want);
             let len: u32 = pts.windows(2).map(|w| ((w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs()) as u32).sum();
             check!(format!("length {pts:?}"), length(&path), len);
         }
     }
     """],
    [("rust", "You may implement a trait only if the trait or the type is local. `Vec<Point>` isn't local even though `Point` is, so wrap it: `pub struct Path(pub Vec<Point>);` and `impl Display for Path`."),
     ("rust", "A newtype loses its inner type's methods. `impl Deref for Path { type Target = Vec<Point>; .. }` plus `DerefMut` gives them back, and `&Path` then coerces to `&[Point]`. `From<Vec<Point>>` and `FromIterator<Point>` cover the two ways of building one.")],
    ("""Coherence needs one crate to own every impl, so the orphan rule says `impl Trait for Type` needs a local trait or a local type (with `Vec<Local>` not counting as local; only `&T`, `&mut T`, `Box<T>` and `Pin<T>` are "fundamental" and see through to `T`). The newtype pattern adds a local type at no runtime cost. `Deref` to the wrapped type is the convenient way to forward its API; some teams prefer explicit methods or `AsRef` so the newtype doesn't leak everything.

Syntax: `pub struct Path(pub Vec<Point>);` · `impl Deref for Path { type Target = Vec<Point>; fn deref(&self) -> &Vec<Point> { &self.0 } }` · `impl FromIterator<Point> for Path { fn from_iter<I: IntoIterator<Item = Point>>(iter: I) -> Self { .. } }`.""", "O(n) to print", "O(1) extra"),
    "When is `Deref` on a newtype a bad idea? What would you expose instead for a `NonEmptyVec<T>`?",
    ["Orphan rule: a local trait or a local type in every impl.", "Newtype + `Deref`/`DerefMut` + `From`/`FromIterator` to keep the ergonomics."],
    related=("L9", "S8"),
    wrong=dict(
        comma_separated=sub(PATH_SOLUTION, 'f.write_str(" -> ")?;', 'f.write_str(", ")?;'),
        empty_prints_nothing=sub(PATH_SOLUTION, '            return f.write_str("(empty)");', "            return Ok(());"),
    ),
))

LABEL_TRAIT = r"""
use std::fmt::Display;

/// How a value is shown in a report.
pub trait Label {
    fn label(&self) -> String;
}
"""

LABEL_CONTAINERS = r"""
/// A list shows its items' labels in brackets: [1, 2, 3].
impl<T: Label> Label for Vec<T> {
    fn label(&self) -> String {
        format!("[{}]", self.iter().map(Label::label).collect::<Vec<_>>().join(", "))
    }
}

/// A missing value shows as "-".
impl<T: Label> Label for Option<T> {
    fn label(&self) -> String {
        match self {
            Some(v) => v.label(),
            None => "-".to_string(),
        }
    }
}
"""

LABEL_BLANKET = r"""
/// Anything printable shows itself with Display.
impl<T: Display> Label for T {
    fn label(&self) -> String {
        self.to_string()
    }
}
"""

LABEL_FIXED = r"""
/// One impl per printable type, instead of a blanket impl over `Display` that would overlap the
/// `Vec` and `Option` impls below (std may add `impl Display for Vec<T>` one day).
macro_rules! label_via_display {
    ($($t:ty),*) => {
        $(
            impl Label for $t {
                fn label(&self) -> String {
                    self.to_string()
                }
            }
        )*
    };
}

label_via_display!(i32, i64, u64, f64, bool, char, str, String);

/// `&str`, `&Vec<..>` and other references show what they point at.
impl<T: Label + ?Sized> Label for &T {
    fn label(&self) -> String {
        (**self).label()
    }
}
"""

LABEL_SOLUTION = LABEL_TRAIT + LABEL_FIXED + LABEL_CONTAINERS

P.append(fix(
    "fix-conflicting-impls", "Fix: a blanket impl that conflicts (E0119)", "hard", "coherence-extension", ["E0119", "coherence", "blanket impls", "?Sized"],
    """
        The impls for `Vec<T>` and `Option<T>` don't compile next to the blanket impl (E0119: std may add
        `impl Display for Vec<T>` in a future version). Keep every behaviour: the tests label `i32`, `i64`, `u64`,
        `f64`, `bool`, `char`, `&str` and `String`, and `Vec`s and `Option`s of any of those, nested any way.

        Types from outside the crate implement `Label` themselves.
    """,
    LABEL_TRAIT + LABEL_BLANKET + LABEL_CONTAINERS,
    LABEL_SOLUTION,
    [T("numbers_and_text", "5, 2.5, \"hi\", String \"s\", 'c', true", '(5i32.label(), 2.5f64.label(), "hi".label(), String::from("s").label(), \'c\'.label(), true.label())',
       '("5".to_string(), "2.5".to_string(), "hi".to_string(), "s".to_string(), "c".to_string(), "true".to_string())'),
     T("vec_of_str", "vec![\"a\", \"b\"]", 'vec!["a", "b"].label()', '"[a, b]"'),
     T("nested", "vec![vec![1], vec![]]", "vec![vec![1i32], vec![]].label()", '"[[1], []]"'),
     T("options", "vec![Some(1), None]", "vec![Some(1i64), None].label()", '"[1, -]"'),
     """
     #[test]
     fn outside_type() {
         struct Money(i64);
         impl Label for Money {
             fn label(&self) -> String {
                 format!("${}", self.0)
             }
         }
         check!("vec![Some(Money(5)), None]", vec![Some(Money(5)), None].label(), "[$5, -]");
     }
     """],
    [T("empty_vec", "Vec::<i32>::new()", "Vec::<i32>::new().label()", '"[]"'),
     T("none_alone", "None::<String>", "None::<String>.label()", '"-"'),
     T("option_of_vec", "Some(vec![1u64, 2])", "Some(vec![1u64, 2]).label()", '"[1, 2]"'),
     T("float_whole", "1.0", "1.0f64.label()", '"1"'),
     T("i64_min", "i64::MIN", "i64::MIN.label()", '"-9223372036854775808"'),
     T("vec_of_strings", "vec![String \"x y\", String \"\"]", 'vec![String::from("x y"), String::new()].label()', '"[x y, ]"'),
     T("deep_nesting", "vec![vec![Some(vec!['a'])], vec![None]]", "vec![vec![Some(vec!['a'])], vec![None]].label()", '"[[[a]], [-]]"'),
     T("unicode_char_and_str", "vec!['é'], \"日本\"", '(vec![\'é\'].label(), "日本".label())', '("[é]".to_string(), "日本".to_string())'),
     T("borrowed_vec", "&vec![true, false]", "(&vec![true, false]).label()", '"[true, false]"'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4418);
         for _ in 0..300 {
             let n = rng.below(4);
             let v: Vec<Option<i32>> = (0..n).map(|_| if rng.bool() { Some(rng.int(-20, 20) as i32) } else { None }).collect();
             let want = format!("[{}]", v.iter().map(|x| x.map_or("-".to_string(), |x| x.to_string())).collect::<Vec<_>>().join(", "));
             check!(format!("{v:?}"), v.label(), want);
         }
     }
     """],
    [("approach", "Two overlapping impls can't both exist, so one side has to go. The containers are the point of the trait; drop the blanket and implement `Label` for each printable type instead (a small `macro_rules!` keeps it short)."),
     ("rust", "`\"hi\".label()` and `vec![\"a\"]` need `str` and `&str` covered: `impl Label for str` plus a forwarding `impl<T: Label + ?Sized> Label for &T`. Neither overlaps `Vec<T>` or `Option<T>`.")],
    ("""Coherence forbids two impls that could apply to the same type. For `impl<T: Display> Label for T` and `impl<T: Label> Label for Vec<T>`, the compiler has to assume `Vec<T>` might implement `Display` one day (std is allowed to add that), so they may overlap. Specialization, which would pick the more specific impl, is unstable. The stable options are concrete impls (often generated by a macro, as std does for numbers), a newtype around one side, or a separate trait.

Syntax: `macro_rules! m { ($($t:ty),*) => { $(impl Label for $t { .. })* }; }` · `impl<T: Label + ?Sized> Label for &T { fn label(&self) -> String { (**self).label() } }`.""", "O(output)", "O(output)"),
    "Why does `impl<T: Display> ToString for T` in std not conflict with anything, while yours does?",
    ["Blanket impls overlap any other impl whose type could meet their bound, even in a future std.", "Concrete impls via a macro, or a newtype, resolve E0119 on stable."],
    related=("L10", "S8"),
    wrong=dict(
        none_as_word=sub(LABEL_SOLUTION, 'None => "-".to_string(),', 'None => "None".to_string(),'),
        comma_without_space=sub(LABEL_SOLUTION, '.join(", ")', '.join(",")'),
    ),
))

ROUTER_HEAD = r"""
use std::collections::BTreeMap;
use std::sync::Arc;

pub trait Handler {
    fn call(&self, req: &str) -> String;

    /// A short description for route listings.
    fn name(&self) -> &'static str {
        "handler"
    }
}

/// Always replies with the same body.
pub struct Static(pub String);

impl Handler for Static {
    fn call(&self, _req: &str) -> String {
        self.0.clone()
    }

    fn name(&self) -> &'static str {
        "static"
    }
}
"""

ROUTER_TAIL = r"""
#[derive(Default)]
pub struct Router {
    routes: BTreeMap<String, Box<dyn Handler + Send + Sync>>,
}

impl Router {
    pub fn route(mut self, path: &str, h: impl Handler + Send + Sync + 'static) -> Self {
        self.routes.insert(path.to_string(), Box::new(h));
        self
    }

    /// The reply of the handler for `path`, or "404 <path>".
    pub fn dispatch(&self, path: &str, req: &str) -> String {
        match self.routes.get(path) {
            Some(h) => h.call(req),
            None => format!("404 {path}"),
        }
    }

    /// "<path>: <handler name>" for every route, sorted by path.
    pub fn describe(&self) -> Vec<String> {
        self.routes.iter().map(|(p, h)| format!("{p}: {}", h.name())).collect()
    }
}
"""

ROUTER_BLANKETS = r"""
/// Any function or closure from a request to a reply is a handler.
impl<F> Handler for F
where
    F: Fn(&str) -> String,
{
    fn call(&self, req: &str) -> String {
        self(req)
    }
}

/// A shared handler is a handler. (A `Box<H>` impl would conflict with the one above:
/// `Box<F>` is itself `Fn` when `F` is.)
impl<H: Handler + ?Sized> Handler for Arc<H> {
    fn call(&self, req: &str) -> String {
        (**self).call(req)
    }

    fn name(&self) -> &'static str {
        (**self).name()
    }
}
"""

ROUTER_SOLUTION = ROUTER_HEAD + ROUTER_BLANKETS + ROUTER_TAIL

P.append(fix(
    "blanket-impls", "Blanket impls: closures and Arcs as handlers", "hard", "coherence-extension", ["blanket impls", "Fn traits", "?Sized", "HRTB"],
    """
        The tests register plain closures (`|req: &str| req.to_uppercase()`), functions, boxed closures, `Arc`s of
        handlers and `Arc<dyn Handler + Send + Sync>` shared by several routes. None of those is a `Handler` yet.
        Add blanket impls so they all are, without changing `Router`. `describe` must show the real handler's
        name through an `Arc`.
    """,
    ROUTER_HEAD + ROUTER_TAIL,
    ROUTER_SOLUTION,
    [T("closure_route", "route /up to |req| req.to_uppercase(); dispatch /up \"hi\"", 'r.dispatch("/up", "hi")', '"HI"',
       setup='let r = Router::default().route("/up", |req: &str| req.to_uppercase());'),
     T("function_route", "route /len to fn len_of", 'r.dispatch("/len", "four")', '"4"',
       setup='fn len_of(req: &str) -> String {\n    req.len().to_string()\n}\nlet r = Router::default().route("/len", len_of);'),
     T("shared_dyn_handler", "one Arc<dyn Handler> on /a and /b", '(r.dispatch("/a", ""), r.dispatch("/b", ""))', '("ok".to_string(), "ok".to_string())',
       setup='let h: Arc<dyn Handler + Send + Sync> = Arc::new(Static("ok".into()));\nlet r = Router::default().route("/a", h.clone()).route("/b", h);'),
     T("describe_through_arc", "routes /s (Arc<Static>), /f (closure)", "r.describe()", 'vec!["/f: handler", "/s: static"]',
       setup='let r = Router::default().route("/s", Arc::new(Static("x".into()))).route("/f", |_: &str| String::new());'),
     T("not_found", "dispatch /nope on an empty router", 'Router::default().dispatch("/nope", "")', '"404 /nope"'),
     "use std::sync::Arc;\n"],
    [T("boxed_closure", "route a Box<closure>", 'r.dispatch("/b", "x")', '"x!"',
       setup='let r = Router::default().route("/b", Box::new(|req: &str| format!("{req}!")));'),
     T("closure_captures_arc_counter", "closure counting calls; 3 dispatches", "hits.load(std::sync::atomic::Ordering::SeqCst)", "3",
       setup='let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));\nlet h2 = hits.clone();\nlet r = Router::default().route("/c", move |_: &str| {\n    h2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);\n    String::new()\n});\nfor _ in 0..3 {\n    r.dispatch("/c", "");\n}'),
     T("arc_of_closure", "Arc::new(closure)", 'r.dispatch("/a", "q")', '"<q>"',
       setup='let r = Router::default().route("/a", Arc::new(|req: &str| format!("<{req}>")));'),
     T("describe_dyn_arc", "Arc<dyn Handler> of a Static", "r.describe()", 'vec!["/x: static"]',
       setup='let h: Arc<dyn Handler + Send + Sync> = Arc::new(Static(String::new()));\nlet r = Router::default().route("/x", h);'),
     T("nested_arc", "Arc<Arc<Static>>", '(r.dispatch("/n", ""), r.describe())', '("deep".to_string(), vec!["/n: static".to_string()])',
       setup='let r = Router::default().route("/n", Arc::new(Arc::new(Static("deep".into()))));'),
     T("replaced_route", "route /r twice", 'r.dispatch("/r", "")', '"second"',
       setup='let r = Router::default().route("/r", Static("first".into())).route("/r", Static("second".into()));'),
     T("unicode_request", "closure echoing the request reversed", 'r.dispatch("/rev", "héllo")', '"olléh"',
       setup='let r = Router::default().route("/rev", |req: &str| req.chars().rev().collect::<String>());'),
     T("dispatch_from_threads", "4 threads dispatch /up", "out", 'vec!["A", "B", "C", "D"]',
       setup='let r = Router::default().route("/up", |req: &str| req.to_uppercase());\nlet mut out: Vec<String> = std::thread::scope(|s| {\n    let hs: Vec<_> = ["a", "b", "c", "d"].iter().map(|q| {\n        let r = &r;\n        s.spawn(move || r.dispatch("/up", q))\n    }).collect();\n    hs.into_iter().map(|h| h.join().unwrap()).collect()\n});\nout.sort();'),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4419);
         for _ in 0..200 {
             let reps = rng.below(3);
             let tag = rng.string(2, "xy");
             let t2 = tag.clone();
             let shared: Arc<dyn Handler + Send + Sync> = Arc::new(Static(tag.clone()));
             let r = Router::default()
                 .route("/rep", move |req: &str| req.repeat(reps))
                 .route("/tag", move |req: &str| format!("{t2}{req}"))
                 .route("/s1", shared.clone())
                 .route("/s2", shared);
             let len = rng.below(4);
             let req = rng.string(len, "ab");
             check!(format!("/rep x{reps} {req:?}"), r.dispatch("/rep", &req), req.repeat(reps));
             check!(format!("/tag {tag:?} {req:?}"), r.dispatch("/tag", &req), format!("{tag}{req}"));
             check!(format!("/s1 and /s2 = {tag:?}"), (r.dispatch("/s1", &req), r.dispatch("/s2", &req)), (tag.clone(), tag.clone()));
         }
     }
     """,
     "use std::sync::Arc;\n"],
    [("rust", "One blanket impl covers every function and closure: `impl<F> Handler for F where F: Fn(&str) -> String`. It also covers `Box<closure>`, because `Box<F>` is `Fn` when `F` is. Don't add a `Box<H>` impl: it would overlap (E0119)."),
     ("rust", "`impl<H: Handler + ?Sized> Handler for Arc<H>` needs `?Sized` for `Arc<dyn Handler + ..>`, and must override `name` too, or the default hides the inner handler's name."),
     ("edge case", "Inside the `Arc` impl, call `(**self).call(req)`. `self.call(req)` picks the Arc impl again and recurses forever.")],
    ("""A blanket impl implements a trait for every type meeting a bound. This is how axum turns async functions into handlers and tower turns closures into services. `Fn(&str) -> String` is sugar for the higher-ranked `for<'a> Fn(&'a str) -> String`: the closure must accept a request of any lifetime. Coherence limits what you can add next to it: `Arc<H>` is fine (std doesn't make `Arc` callable), `Box<H>` isn't (std does). Forwarding impls must forward every default method too.

Syntax: `impl<F> Handler for F where F: Fn(&str) -> String { fn call(&self, req: &str) -> String { self(req) } }` · `impl<H: Handler + ?Sized> Handler for Arc<H> { .. }`.""", "O(log routes) per dispatch", "O(routes)"),
    "axum handlers are async and take extractors. What does the blanket impl look like for `Fn(Request) -> Fut where Fut: Future<Output = Response>`?",
    ["Blanket impls over `Fn` turn closures into trait implementors.", "A `Box<H>` forwarding impl overlaps a blanket impl over `Fn`.", "Forwarding impls must forward default methods too."],
    related=("L6", "B2"),
    wrong=dict(
        arc_hides_name=sub(ROUTER_SOLUTION, "\n    fn name(&self) -> &'static str {\n        (**self).name()\n    }\n", "\n"),
        arc_recurses=sub(ROUTER_SOLUTION, "(**self).call(req)", "self.call(req)"),
    ),
))

DECODE_HEAD = r"""
/// Parses a value from text that lives for `'de`. Types that borrow from the text (`&'de str`) and types
/// that own their data (`u32`, `String`, `Vec<u32>`) can both implement it.
pub trait Decode<'de>: Sized {
    fn decode(input: &'de str) -> Option<Self>;
}

impl<'de> Decode<'de> for &'de str {
    fn decode(input: &'de str) -> Option<Self> {
        (!input.is_empty()).then_some(input)
    }
}

impl<'de> Decode<'de> for u32 {
    fn decode(input: &'de str) -> Option<Self> {
        input.parse().ok()
    }
}

impl<'de> Decode<'de> for String {
    fn decode(input: &'de str) -> Option<Self> {
        Some(input.to_string())
    }
}

/// Comma-separated items; an empty input is an empty list, and any bad item fails the whole list.
impl<'de, T: Decode<'de>> Decode<'de> for Vec<T> {
    fn decode(input: &'de str) -> Option<Self> {
        if input.is_empty() {
            return Some(Vec::new());
        }
        input.split(',').map(T::decode).collect()
    }
}

/// Decodes each line exactly as written, borrowing from `text` where the type allows.
pub fn decode_lines<'de, T: Decode<'de>>(text: &'de str) -> Vec<Option<T>> {
    text.lines().map(T::decode).collect()
}
"""

DECODE_OWNED = r"""
/// Types that decode from text of any lifetime, so they never borrow from it. Like serde's DeserializeOwned.
pub trait DecodeOwned: for<'de> Decode<'de> {}

impl<T> DecodeOwned for T where T: for<'de> Decode<'de> {}
"""

DECODE_NORMALIZED = r"""
/// Decodes each line after trimming and lowercasing it. The cleaned lines are temporary, so only types that
/// own their data can come out.
pub fn decode_normalized<BOUND>(text: &str) -> Vec<Option<T>> {
    text.lines()
        .map(|line| {
            let clean = line.trim().to_lowercase();
            T::decode(&clean)
        })
        .collect()
}
"""

DECODE_SOLUTION = DECODE_HEAD + DECODE_OWNED + DECODE_NORMALIZED.replace("<BOUND>", "<T: DecodeOwned>")

P.append(fix(
    "fix-hrtb-decode-owned", "Fix: a lifetime chosen by the caller (HRTB)", "hard", "coherence-extension", ["HRTB", "for<'a>", "E0597", "DeserializeOwned"],
    """
        `decode_normalized` doesn't compile: `clean` does not live long enough (E0597). Fix its bound.

        Also add `DecodeOwned`, a trait for "decodes from text of any lifetime" (like serde's
        `DeserializeOwned`), implemented automatically for every such type, so callers can write `T: DecodeOwned`.
    """,
    DECODE_HEAD + DECODE_NORMALIZED.replace("<BOUND>", "<'de, T: Decode<'de>>"),
    DECODE_SOLUTION,
    [T("normalized_numbers", "decode_normalized::<u32>(\" 42 \\nx\\n7\")", 'decode_normalized::<u32>(" 42 \\nx\\n7")', "vec![Some(42), None, Some(7)]"),
     T("normalized_strings", "decode_normalized::<String>(\"  HeLLo \")", 'decode_normalized::<String>("  HeLLo ")', 'vec![Some("hello".to_string())]'),
     T("normalized_lists", "decode_normalized::<Vec<u32>>(\"1,2\\n3,x\")", 'decode_normalized::<Vec<u32>>("1,2\\n3,x")', "vec![Some(vec![1, 2]), None]"),
     T("borrowed_lines_point_into_text", "decode_lines::<&str>(\"ab\\ncd\")", "std::ptr::eq(got[1].unwrap().as_ptr(), text[3..].as_ptr())", "true",
       setup='let text = String::from("ab\\ncd");\nlet got = decode_lines::<&str>(&text);'),
     """
     #[test]
     fn decode_owned_bound() {
         // With only `T: DecodeOwned`, T must decode from a local String.
         fn from_temp<T: DecodeOwned>(s: &str) -> Option<T> {
             let tmp = format!("{s}0");
             T::decode(&tmp)
         }
         check!("from_temp::<u32>(\\"1\\")", from_temp::<u32>("1"), Some(10));
         check!("from_temp::<Vec<u32>>(\\"5,\\")", from_temp::<Vec<u32>>("5,"), Some(vec![5, 0]));
     }
     """],
    [T("empty_text", "decode_normalized::<u32>(\"\")", 'decode_normalized::<u32>("")', "Vec::<Option<u32>>::new()"),
     T("empty_line_is_empty_list", "decode_normalized::<Vec<u32>>(\"\\n1\")", 'decode_normalized::<Vec<u32>>("\\n1")', "vec![Some(vec![]), Some(vec![1])]"),
     T("uppercase_unicode", "decode_normalized::<String>(\"ÉTÉ\")", 'decode_normalized::<String>("ÉTÉ")', 'vec![Some("été".to_string())]'),
     T("u32_overflow", "decode_normalized::<u32>(\"4294967296\")", 'decode_normalized::<u32>("4294967296")', "vec![None]"),
     T("u32_max", "decode_normalized::<u32>(\"4294967295\")", 'decode_normalized::<u32>("4294967295")', "vec![Some(u32::MAX)]"),
     T("lines_unchanged", "decode_lines::<String>(\" A \")", 'decode_lines::<String>(" A ")', 'vec![Some(" A ".to_string())]'),
     T("borrowed_empty_line", "decode_lines::<&str>(\"a\\n\\nb\")", 'decode_lines::<&str>("a\\n\\nb")', 'vec![Some("a"), None, Some("b")]'),
     T("borrowed_list", "decode_lines::<Vec<&str>>(\"x,y\")", 'decode_lines::<Vec<&str>>("x,y")', 'vec![Some(vec!["x", "y"])]'),
     T("list_with_empty_item", "decode_normalized::<Vec<u32>>(\"1,,2\")", 'decode_normalized::<Vec<u32>>("1,,2")', "vec![None]"),
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4420);
         for _ in 0..300 {
             let n = rng.below(4);
             let lines: Vec<String> = (0..n).map(|_| {
                 let pad = rng.below(2);
                 let body = if rng.below(5) == 0 { "X".to_string() } else { rng.int(0, 999).to_string() };
                 format!("{}{}{}", " ".repeat(pad), body, " ".repeat(pad))
             }).collect();
             let text = lines.join("\n");
             let want: Vec<Option<u32>> = lines.iter().map(|l| l.trim().to_lowercase().parse().ok()).collect();
             check!(format!("{text:?}"), decode_normalized::<u32>(&text), want);
         }
     }
     """],
    [("approach", "With `<'de, T: Decode<'de>>`, the caller picks `'de`, so it must outlive the call, and a local `String` can't be borrowed for it. You need \"T decodes from text of *every* lifetime\"."),
     ("rust", "That's a higher-ranked bound: `T: for<'de> Decode<'de>`. Make it a trait with a blanket impl so it has a name: `trait DecodeOwned: for<'de> Decode<'de> {}` and `impl<T> DecodeOwned for T where T: for<'de> Decode<'de> {}`.")],
    ("""A lifetime parameter on a function is chosen by the caller, and every borrow passed where `'de` is expected must last that long, so a value created inside the function can't be used. `for<'de>` flips it: the bound must hold for all lifetimes, including the short one of a local. `&'de str` implements `Decode<'de>` only for its own `'de`, so it isn't `DecodeOwned`, which is exactly why borrowed types are rejected. serde's `DeserializeOwned` is this same trait-plus-blanket-impl.

Syntax: `where T: for<'de> Decode<'de>` · `pub trait DecodeOwned: for<'de> Decode<'de> {}` · `impl<T> DecodeOwned for T where T: for<'de> Decode<'de> {}`.""", "O(text)", "O(line)"),
    "Why can't `decode_normalized::<&str>` compile after the fix? What would you return to allow borrowed output there?",
    ["A lifetime parameter is picked by the caller; locals can't satisfy it.", "`for<'a>` bounds hold for every lifetime.", "Name an HRTB with a trait + blanket impl (`DeserializeOwned`)."],
    related=("L3", "L5"),
    wrong=dict(
        no_trim=sub(DECODE_SOLUTION, "let clean = line.trim().to_lowercase();", "let clean = line.to_lowercase();"),
        list_skips_bad_items=sub(DECODE_SOLUTION, "input.split(',').map(T::decode).collect()", "Some(input.split(',').filter_map(T::decode).collect())"),
    ),
))

ITEREXT_SOLUTION = r"""
use std::collections::HashMap;
use std::hash::Hash;

/// Extra adapters for every iterator.
pub trait IterExt: Iterator + Sized {
    /// Items 0, n, 2n, ... Panics if `n` is 0.
    fn every_nth(self, n: usize) -> EveryNth<Self> {
        assert!(n > 0, "every_nth(0)");
        EveryNth { iter: self, n, first: true }
    }

    /// Drops each item equal to the item just before it.
    fn dedup_adjacent(self) -> DedupAdjacent<Self>
    where
        Self::Item: PartialEq + Clone,
    {
        DedupAdjacent { iter: self, last: None }
    }

    /// How many times each item occurs.
    fn counts(self) -> HashMap<Self::Item, usize>
    where
        Self::Item: Eq + Hash,
    {
        let mut m = HashMap::new();
        for x in self {
            *m.entry(x).or_insert(0) += 1;
        }
        m
    }
}

impl<I: Iterator> IterExt for I {}

pub struct EveryNth<I> {
    iter: I,
    n: usize,
    first: bool,
}

impl<I: Iterator> Iterator for EveryNth<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        if self.first {
            self.first = false;
            self.iter.next()
        } else {
            // nth lets the inner iterator skip in O(1) when it can (ranges, slices).
            self.iter.nth(self.n - 1)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (lo, hi) = self.iter.size_hint();
        let f = |x: usize| if self.first { x.div_ceil(self.n) } else { x / self.n };
        (f(lo), hi.map(f))
    }
}

pub struct DedupAdjacent<I: Iterator> {
    iter: I,
    last: Option<I::Item>,
}

impl<I> Iterator for DedupAdjacent<I>
where
    I: Iterator,
    I::Item: PartialEq + Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        loop {
            let x = self.iter.next()?;
            if self.last.as_ref() != Some(&x) {
                self.last = Some(x.clone());
                return Some(x);
            }
        }
    }
}
"""

ITEREXT_STARTER = r"""
use std::collections::HashMap;
use std::hash::Hash;

/// Extra adapters for every iterator.
pub trait IterExt: Iterator + Sized {
    /// Items 0, n, 2n, ... Panics if `n` is 0.
    fn every_nth(self, n: usize) -> EveryNth<Self> {
        todo!()
    }

    /// Drops each item equal to the item just before it.
    fn dedup_adjacent(self) -> DedupAdjacent<Self>
    where
        Self::Item: PartialEq + Clone,
    {
        todo!()
    }

    /// How many times each item occurs.
    fn counts(self) -> HashMap<Self::Item, usize>
    where
        Self::Item: Eq + Hash,
    {
        todo!()
    }
}

impl<I: Iterator> IterExt for I {}

pub struct EveryNth<I> {
    iter: I,
    // TODO: more fields
}

impl<I: Iterator> Iterator for EveryNth<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        todo!()
    }
}

pub struct DedupAdjacent<I: Iterator> {
    iter: I,
    // TODO: more fields
}

impl<I> Iterator for DedupAdjacent<I>
where
    I: Iterator,
    I::Item: PartialEq + Clone,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<I::Item> {
        todo!()
    }
}
"""

P.append(write(
    "iterator-extension", "An extension trait on Iterator", "hard", "coherence-extension", ["extension traits", "blanket impls", "iterator adapters", "size_hint"],
    """
        `IterExt` adds three methods to every iterator through a blanket impl. Implement them:

        - `every_nth(n)`: items 0, n, 2n, …; panics if `n` is 0. It must skip as fast as the inner iterator can:
          `(0u64..).every_nth(1_000_000_000_000)` has to be instant. Its `size_hint` must be exact whenever the
          inner one is.
        - `dedup_adjacent()`: drops each item equal to the one just before it.
        - `counts()`: how many times each item occurs.

        The adapters are lazy: they pull only the items they need.
    """,
    ITEREXT_STARTER,
    ITEREXT_SOLUTION,
    [T("every_third", "(1..=10).every_nth(3)", "(1..=10).every_nth(3).collect::<Vec<_>>()", "vec![1, 4, 7, 10]"),
     T("dedup_chars", "\"aabbbca\".chars().dedup_adjacent()", '"aabbbca".chars().dedup_adjacent().collect::<String>()', '"abca"'),
     T("counts_words", "\"a b a c a\".split(' ').counts(), sorted", 'sorted', 'vec![("a", 3), ("b", 1), ("c", 1)]',
       setup='let mut sorted: Vec<_> = "a b a c a".split(\' \').counts().into_iter().collect();\nsorted.sort();'),
     T("huge_skips", "(0u64..).every_nth(1_000_000_000_000).take(3)", "(0u64..).every_nth(1_000_000_000_000).take(3).collect::<Vec<_>>()", "vec![0, 1_000_000_000_000, 2_000_000_000_000]"),
     T("exact_size_hint", "(0..10).every_nth(3).size_hint()", "(0..10).every_nth(3).size_hint()", "(4, Some(4))"),
     T("on_references", "v.iter().every_nth(2) over [\"x\", \"y\", \"z\"]", "v.iter().every_nth(2).collect::<Vec<_>>()", 'vec![&"x", &"z"]', setup='let v = ["x", "y", "z"];')],
    [T("every_first", "(0..5).every_nth(1)", "(0..5).every_nth(1).collect::<Vec<_>>()", "vec![0, 1, 2, 3, 4]"),
     T("n_past_the_end", "(0..3).every_nth(10)", "(0..3).every_nth(10).collect::<Vec<_>>()", "vec![0]"),
     T("empty", "(0..0).every_nth(2)", "(0..0).every_nth(2).count()", "0"),
     T("zero_panics", "(0..5).every_nth(0)", "std::panic::catch_unwind(|| (0..5).every_nth(0).count()).is_err()", "true"),
     T("size_hint_after_next", "(0..10).every_nth(3) after one next", "(it.size_hint(), it.count())", "((3, Some(3)), 3)", setup="let mut it = (0..10).every_nth(3);\nit.next();"),
     T("size_hint_exact_multiple", "(0..9).every_nth(3)", "(0..9).every_nth(3).size_hint()", "(3, Some(3))"),
     T("size_hint_unbounded", "(0u64..).every_nth(2), upper bound", "(0u64..).every_nth(2).size_hint().1", "None"),
     T("dedup_all_same", "[7, 7, 7]", "[7, 7, 7].into_iter().dedup_adjacent().collect::<Vec<_>>()", "vec![7]"),
     T("dedup_alternating", "[1, 2, 1, 2]", "[1, 2, 1, 2].into_iter().dedup_adjacent().collect::<Vec<_>>()", "vec![1, 2, 1, 2]"),
     T("dedup_lazy_on_infinite", "(0u64..).map(|x| x / 3).dedup_adjacent().take(4)", "(0u64..).map(|x| x / 3).dedup_adjacent().take(4).collect::<Vec<_>>()", "vec![0, 1, 2, 3]"),
     T("every_nth_pulls_only_what_it_needs", "(0..100) with a pull counter, every_nth(3).take(2)", "(got, pulled.get())", "(vec![0, 3], 4)",
       setup="let pulled = std::cell::Cell::new(0);\nlet got: Vec<i32> = (0..100).inspect(|_| pulled.set(pulled.get() + 1)).every_nth(3).take(2).collect();"),
     T("counts_empty", "std::iter::empty::<u8>().counts()", "std::iter::empty::<u8>().counts().len()", "0"),
     """
     #[test]
     fn works_on_a_custom_iterator() {
         struct Fib(u64, u64);
         impl Iterator for Fib {
             type Item = u64;
             fn next(&mut self) -> Option<u64> {
                 let x = self.0;
                 *self = Fib(self.1, self.0 + self.1);
                 Some(x)
             }
         }
         check!("Fib.every_nth(2).take(5)", Fib(0, 1).every_nth(2).take(5).collect::<Vec<_>>(), vec![0, 1, 3, 8, 21]);
         check!("Fib.dedup_adjacent().take(4)", Fib(0, 1).dedup_adjacent().take(4).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4421);
         for _ in 0..300 {
             let len = rng.below(12);
             let v: Vec<u8> = rng.vec(len, 0, 2);
             let n = rng.below(4) + 1;
             check!(format!("{v:?}.every_nth({n})"), v.iter().copied().every_nth(n).collect::<Vec<_>>(), v.iter().copied().step_by(n).collect::<Vec<_>>());
             let skip = rng.below(len + 1);
             let mut it = v.iter().every_nth(n);
             for _ in 0..skip.min(2) {
                 it.next();
             }
             let hint = it.size_hint();
             let rest = it.count();
             check!(format!("{v:?}.every_nth({n}) size_hint after {} next", skip.min(2)), hint, (rest, Some(rest)));
             let mut d = v.clone();
             d.dedup();
             check!(format!("{v:?}.dedup_adjacent()"), v.iter().copied().dedup_adjacent().collect::<Vec<_>>(), d);
             let mut c: Vec<(u8, usize)> = v.iter().copied().counts().into_iter().collect();
             c.sort();
             let want: Vec<(u8, usize)> = (0..=2).map(|k| (k, v.iter().filter(|&&x| x == k).count())).filter(|&(_, n)| n > 0).collect();
             check!(format!("{v:?}.counts()"), c, want);
         }
     }
     """],
    [("rust", "`impl<I: Iterator> IterExt for I {}` gives every iterator the defaults; each method just builds its adapter struct. The adapters store the inner iterator and the state they need (`n` and whether the first item is out; the last item yielded)."),
     ("approach", "After the first item, `self.iter.nth(n - 1)` skips `n - 1` items and returns the next. Ranges implement `nth` in O(1); a loop of `next()` calls takes a trillion steps."),
     ("edge case", "`size_hint`: before the first item, `len.div_ceil(n)` items remain; after it, `len / n`. Apply the same map to both bounds.")],
    ("""An extension trait adds methods to a foreign trait's implementors: declare `trait IterExt: Iterator`, give it default methods, and blanket-implement it for every `I: Iterator`. Callers opt in with `use` (as with `itertools::Itertools`). Good adapters are lazy, forward `size_hint`, and use the inner iterator's specialised methods (`nth`) rather than re-implementing them with `next`.

Syntax: `pub trait IterExt: Iterator + Sized { fn every_nth(self, n: usize) -> EveryNth<Self> { .. } }` · `impl<I: Iterator> IterExt for I {}` · a method-level bound: `fn counts(self) -> HashMap<Self::Item, usize> where Self::Item: Eq + Hash`.""", "O(1) amortised per item (O(1) skips on ranges)", "O(1) per adapter; O(distinct) for counts"),
    "`itertools` names its trait `Itertools`, not `IteratorExt`. What happens if std later adds a method with the same name as one of yours?",
    ["Extension trait = trait with defaults + blanket impl over the base trait.", "Delegate to `nth` so skipping is as fast as the inner iterator allows.", "Forward `size_hint`."],
    related=("S6", "L5"),
    wrong=dict(
        skips_with_next_loop=sub(ITEREXT_SOLUTION, "            self.iter.nth(self.n - 1)",
                                 "            for _ in 1..self.n {\n                self.iter.next()?;\n            }\n            self.iter.next()"),
        starts_at_n=sub(ITEREXT_SOLUTION, "        if self.first {\n            self.first = false;\n            self.iter.next()\n        } else {", "        if self.first {\n            self.first = false;\n            self.iter.nth(self.n - 1)\n        } else {"),
        global_dedup=sub(sub(sub(ITEREXT_SOLUTION, "    last: Option<I::Item>,\n}", "    seen: Vec<I::Item>,\n}"),
                             "DedupAdjacent { iter: self, last: None }", "DedupAdjacent { iter: self, seen: Vec::new() }"),
                         "            if self.last.as_ref() != Some(&x) {\n                self.last = Some(x.clone());", "            if !self.seen.contains(&x) {\n                self.seen.push(x.clone());"),
    ),
))

LENDING_TRAIT = r"""
/// An iterator whose items may borrow from the iterator itself. Each item must be dropped before the
/// next call to `next`, which `std::iter::Iterator` can't express.
pub trait LendingIterator {
    type Item<'a>
    where
        Self: 'a;

    fn next(&mut self) -> Option<Self::Item<'_>>;
}
"""

LENDING_BODY = r"""
/// Overlapping mutable windows of `size`, moving one element at a time.
pub struct WindowsMut<'s, T> {
    slice: &'s mut [T],
    size: usize,
    start: usize,
}

/// Panics if `size` is 0.
pub fn windows_mut<T>(slice: &mut [T], size: usize) -> WindowsMut<'_, T> {
    assert!(size > 0, "window size 0");
    WindowsMut { slice, size, start: 0 }
}

impl<'s, T> LendingIterator for WindowsMut<'s, T> {
    type Item<'a>
        = &'a mut [T]
    where
        Self: 'a;

    fn next(&mut self) -> Option<&mut [T]> {
        let end = self.start + self.size;
        if end > self.slice.len() {
            return None;
        }
        let window = &mut self.slice[self.start..end];
        self.start += 1;
        Some(window)
    }
}

/// Each line of `text`, trimmed and uppercased, written into one reused buffer.
pub struct UpperLines<'s> {
    lines: std::str::Lines<'s>,
    buf: String,
}

pub fn upper_lines(text: &str) -> UpperLines<'_> {
    UpperLines { lines: text.lines(), buf: String::new() }
}

impl<'s> LendingIterator for UpperLines<'s> {
    type Item<'a>
        = &'a str
    where
        Self: 'a;

    fn next(&mut self) -> Option<&str> {
        let line = self.lines.next()?;
        self.buf.clear();
        for c in line.trim().chars() {
            self.buf.extend(c.to_uppercase());
        }
        Some(&self.buf)
    }
}

/// Counts the items of any lending iterator.
pub fn count<L: LendingIterator>(mut it: L) -> usize {
    let mut n = 0;
    while it.next().is_some() {
        n += 1;
    }
    n
}
"""

LENDING_STARTER = r"""
use std::marker::PhantomData;

// TODO: the LendingIterator trait. Its `next` returns an item that may borrow from the iterator itself.

/// Overlapping mutable windows of `size`, moving one element at a time.
pub struct WindowsMut<'s, T> {
    // TODO (remove the placeholder)
    _todo: PhantomData<&'s mut T>,
}

/// Panics if `size` is 0.
pub fn windows_mut<T>(slice: &mut [T], size: usize) -> WindowsMut<'_, T> {
    todo!()
}

impl<'s, T> LendingIterator for WindowsMut<'s, T> {
    // TODO: items are `&mut [T]` windows.
}

/// Each line of `text`, trimmed and uppercased, written into one reused buffer.
pub struct UpperLines<'s> {
    // TODO (remove the placeholder)
    _todo: PhantomData<&'s str>,
}

pub fn upper_lines(text: &str) -> UpperLines<'_> {
    todo!()
}

impl<'s> LendingIterator for UpperLines<'s> {
    // TODO: items are `&str` borrowed from the buffer.
}

/// Counts the items of any lending iterator.
pub fn count<L: LendingIterator>(it: L) -> usize {
    todo!()
}
"""

LENDING_SOLUTION = LENDING_TRAIT + LENDING_BODY

LENDING_HELP = r"""
fn collect_upper(text: &str) -> Vec<String> {
    let mut it = upper_lines(text);
    let mut out = Vec::new();
    while let Some(s) = it.next() {
        out.push(s.to_string());
    }
    out
}
"""

P.append(fix(
    "lending-iterator", "A GAT-based LendingIterator", "hard", "coherence-extension", ["GATs", "lending iterator", "reborrowing"],
    """
        Write the `LendingIterator` trait: like `Iterator`, but `next(&mut self)` returns an item that may borrow
        from the iterator itself, through a generic associated type `Item<'a>`. Then implement it:

        - `windows_mut(slice, size)`: overlapping `&mut [T]` windows, moving one element at a time (what
          `slice.windows` does, but mutable). Panics if `size` is 0.
        - `upper_lines(text)`: each line trimmed and uppercased, written into one `String` buffer that is reused
          for every line.
        - `count` works for any lending iterator.
    """,
    LENDING_STARTER,
    LENDING_SOLUTION,
    [LENDING_HELP,
     T("prefix_sums", "windows of 2 over [1, 2, 3, 4], w[1] += w[0]", "v", "vec![1, 3, 6, 10]",
       setup="let mut v = vec![1, 2, 3, 4];\nlet mut it = windows_mut(&mut v, 2);\nwhile let Some(w) = it.next() {\n    w[1] += w[0];\n}"),
     T("window_count", "count(windows_mut(&mut [0; 5], 3))", "count(windows_mut(&mut [0; 5], 3))", "3"),
     T("too_big", "count(windows_mut(&mut [1, 2], 3))", "count(windows_mut(&mut [1, 2], 3))", "0"),
     T("upper", "upper_lines(\" ab\\ncd \")", 'collect_upper(" ab\\ncd ")', 'vec!["AB", "CD"]'),
     T("upper_count", "count(upper_lines(\"a\\nb\\nc\"))", 'count(upper_lines("a\\nb\\nc"))', "3")],
    [LENDING_HELP,
     T("reuses_one_buffer", "addresses of the first and second item of \"hello\\nhi\"", "p1 == p2", "true",
       setup='let mut it = upper_lines("hello\\nhi");\nlet p1 = it.next().unwrap().as_ptr() as usize;\nlet p2 = it.next().unwrap().as_ptr() as usize;'),
     T("whole_slice_window", "windows of 3 over [1, 2, 3]", "count(windows_mut(&mut [1, 2, 3], 3))", "1"),
     T("empty_slice", "windows of 1 over []", "count(windows_mut(&mut Vec::<i32>::new(), 1))", "0"),
     T("zero_size_panics", "windows_mut(&mut [1], 0)", "std::panic::catch_unwind(|| count(windows_mut(&mut [1], 0))).is_err()", "true"),
     T("windows_see_earlier_writes", "windows of 3 over [1, 0, 0, 0, 0]: w[2] = w[0] + w[1] + 1", "v", "vec![1, 0, 2, 3, 6]",
       setup="let mut v = vec![1, 0, 0, 0, 0];\nlet mut it = windows_mut(&mut v, 3);\nwhile let Some(w) = it.next() {\n    w[2] = w[0] + w[1] + 1;\n}"),
     T("upper_unicode", "upper_lines(\"straße\\n  é \")", 'collect_upper("straße\\n  é ")', 'vec!["STRASSE", "É"]'),
     T("upper_empty_lines", "upper_lines(\"a\\n\\n b\")", 'collect_upper("a\\n\\n b")', 'vec!["A", "", "B"]'),
     T("upper_no_text", "upper_lines(\"\")", 'count(upper_lines(""))', "0"),
     """
     #[test]
     fn a_new_lending_iterator() {
         // count accepts any implementation, including items that don't borrow at all.
         struct Countdown(u32);
         impl LendingIterator for Countdown {
             type Item<'a> = u32 where Self: 'a;
             fn next(&mut self) -> Option<u32> {
                 self.0 = self.0.checked_sub(1)?;
                 Some(self.0)
             }
         }
         check!("count(Countdown(4))", count(Countdown(4)), 4);
     }
     """,
     r"""
     #[test]
     fn random_vs_brute_force() {
         let mut rng = anneal_prelude::Rng::new(4422);
         for _ in 0..300 {
             let len = rng.below(8);
             let orig: Vec<i64> = rng.vec(len, -5, 5);
             let size = rng.below(4) + 1;
             let mut v = orig.clone();
             let mut it = windows_mut(&mut v, size);
             while let Some(w) = it.next() {
                 w[size - 1] += w[0] * 2;
             }
             let mut want = orig.clone();
             for s in 0..(len + 1).saturating_sub(size) {
                 want[s + size - 1] += want[s] * 2;
             }
             check!(format!("{orig:?}, size {size}"), v, want);
         }
     }
     """],
    [("rust", "The item type needs its own lifetime, tied to the `&mut self` of each call: `type Item<'a> where Self: 'a;` and `fn next(&mut self) -> Option<Self::Item<'_>>`. The `where Self: 'a` is required: an item can't outlive the iterator it borrows."),
     ("rust", "In `WindowsMut::next`, return `&mut self.slice[start..end]`: a reborrow through `&mut self`, which is exactly what `Item<'_>` allows and `Iterator` would reject. For `UpperLines`, `clear()` the buffer and push into it, so its allocation is kept."),
     ("edge case", "The last window ends exactly at `len`; a window bigger than the slice yields nothing.")],
    ("""`Iterator::Item` can't mention the lifetime of `&mut self` in `next`, so an iterator can't lend out something it owns or hand out overlapping `&mut` windows. A generic associated type adds that lifetime: `Item<'a>` is a different type for each borrow, and callers must drop one item before asking for the next. GATs are also the reason `for` loops and std adapters don't work here, and a generic `for_each<F: for<'a> FnMut(L::Item<'a>)>` currently forces `L: 'static` (a known limitation).

Syntax: `trait LendingIterator { type Item<'a> where Self: 'a; fn next(&mut self) -> Option<Self::Item<'_>>; }` · `impl<'s, T> LendingIterator for WindowsMut<'s, T> { type Item<'a> = &'a mut [T] where Self: 'a; .. }`.""", "O(1) per window; O(line) per line", "O(1) extra; one buffer"),
    "Write `for_each` for a `LendingIterator`. Why does the obvious `F: for<'a> FnMut(L::Item<'a>)` bound demand `'static` here, and how do crates like `lending-iterator` work around it?",
    ["A GAT gives an associated type its own lifetime parameter.", "`where Self: 'a` on the GAT: items can't outlive the iterator.", "Lending iterators allow overlapping `&mut` windows and buffer reuse."],
    related=("S6", "L3"),
    wrong=dict(
        misses_last_window=sub(LENDING_SOLUTION, "if end > self.slice.len() {", "if end >= self.slice.len() {"),
        chunks_not_windows=sub(LENDING_SOLUTION, "self.start += 1;", "self.start += self.size;"),
        fresh_string_per_line=sub(LENDING_SOLUTION, "        self.buf.clear();\n        for c in line.trim().chars() {\n            self.buf.extend(c.to_uppercase());\n        }", "        self.buf = line.trim().to_uppercase();"),
    ),
))

STAGES = [
    ("define-implement", "Define & implement", "easy"),
    ("static-vs-dynamic", "Static vs dynamic", "medium"),
    ("object-safety", "Object safety", "medium"),
    ("operator-traits", "Operator traits", "medium"),
    ("coherence-extension", "Coherence & extension", "hard"),
]

# `source` and `examples` are optional; drop empty ones so problem.toml stays tidy.
for p in P:
    if not p.get("source"):
        p.pop("source", None)
    if p.get("rules") is None:
        p.pop("rules", None)
    if p.get("wrong") is None:
        p.pop("wrong", None)

if __name__ == "__main__":
    n = write_track("l4-traits-dispatch", "L4", "Traits & dispatch", "L", "core", 4,
                    "Traits say what a type can do. Generics pick the code at compile time; trait objects pick it at run time.",
                    STAGES, P)
    print("L4", n)
