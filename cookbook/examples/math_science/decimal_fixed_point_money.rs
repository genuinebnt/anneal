//! RECIPE 893 (math_science): money done right: integer minor units, decimal fixed point, rounding modes, allocation, ledgers.
//! LEVEL: intermediate
//! Use in: billing, invoicing, payments, accounting ledgers, tax and interest calculation, anything where "the cents must add up".   Run: cargo run --release --example decimal_fixed_point_money
//! COMMON USES: payment processors (amounts as integer minor units), invoice/tax engines (per-line vs per-invoice rounding rules), double-entry ledgers and bank cores, splitting a bill or a fee between parties, currency conversion with differing exponents (JPY 0, USD 2, KWD 3).
//! Course: Fowler, "Patterns of Enterprise Application Architecture" (Money pattern, allocation); double-entry bookkeeping as an invariant (sum of postings is zero).
#![allow(dead_code)]
//!
//! WHAT: a small money library plus the property tests that justify each design choice.
//!   * `Currency` carries its exponent (minor-unit digits); `Money` is an `i64` of minor units. No floats anywhere.
//!   * `Decimal { coef: i128, scale }` is decimal fixed point (value = coef / 10^scale) with checked add/sub/mul/div
//!     and a typed `MoneyError` instead of silent wrap or panics. Division takes an explicit target scale AND rounding mode:
//!     there is no "default" rounding, because the legally correct mode depends on the jurisdiction and contract.
//!   * `div_round` implements HalfUp (ties away from zero), HalfEven (banker's), Floor, Ceil and TowardZero once, and is
//!     verified EXHAUSTIVELY for n in -60..=60 and every divisor +-1..=12 against a floating-point floor and an exact tie test.
//!   * `allocate` splits an amount by weights with the LARGEST REMAINDER method so the shares sum EXACTLY to the total
//!     (3000 random cases: sum equals total, every share is floor or ceil of the exact share, sign symmetric).
//!   * Order pitfalls: rounding each invoice line vs rounding the invoice total; (a*b)*c vs a*(b*c) after rounding; compound
//!     interest rounded each period; HalfUp vs HalfEven bias on 1000 ties.
//!   * Formatting/parsing with locales-lite (US `1,234.56`, DE `1.234,56 EUR`, JPY with no decimals); parse refuses
//!     input with more fraction digits than the currency allows rather than silently rounding.
//!   * A double-entry `Ledger`: every entry must have postings that sum to zero; posting is atomic (a failed entry changes
//!     nothing, including on overflow), so the sum of all balances is always zero, checked over 3000 random entries.
//!   * f64 comparison: 0.01 added a million times drifts, `format!("{:.2}", 1.005)` gives 1.00, and counted disagreements with decimal.
//! WHY: a double cannot represent 0.10 (see `floating_point_ieee754`), and the errors are small enough to pass casual
//! testing and large enough to fail an audit. Integers are exact, fast and hashable; the only decisions left are rounding
//! mode and WHERE to round, which must be written down and tested.
//! Read: https://martinfowler.com/eaaCatalog/money.html and https://en.wikipedia.org/wiki/Rounding and
//! https://en.wikipedia.org/wiki/Quota_method (largest remainder) and https://en.wikipedia.org/wiki/Double-entry_bookkeeping
//! TRY NEXT:
//!   - [S] Add `HalfDown` and `AwayFromZero` to `Round`; predict which of the 5 existing modes each equals on ties for negative numbers, then extend `exhaustive_rounding` to prove it.
//!   - [S] Predict whether `allocate` with weights [1,1,1] of 100 gives [34,33,33] or [33,33,34], then change the tie-break in the sort to "highest index first" and see which invariants still hold (which ones are about fairness, which about the sum?).
//!   - [M] Add `Money::checked_convert_via(rate_table, mode)` for triangular conversion USD -> EUR -> JPY and measure the round-trip loss in minor units over 3000 random amounts; what is the max loss for JPY (exp 0)?
//!   - [M] Make `Ledger::post` return the per-account balances before/after as an audit record, and add an `undo(entry_id)` that posts the reversed entry; assert total stays 0 and balances return to the original.
//!   - [A] PROJECT: combine with `amortized_analysis_online_algorithms` or `parallel_prefix_scan_reduction`: build a bank statement that computes running balances with a parallel prefix scan over i64 postings, and assert it equals the sequential `Ledger` balances (integers make the parallel result bit-identical, unlike floats).
use std::collections::BTreeMap;
use std::fmt;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

// ============================================================ errors and rounding
#[derive(Debug, Clone, PartialEq, Eq)]
enum MoneyError {
    Overflow,
    DivisionByZero,
    CurrencyMismatch(&'static str, &'static str),
    Parse(String),
    Unbalanced { sum: i128 },
    NoWeights,
}
impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MoneyError::Overflow => write!(f, "arithmetic overflow"),
            MoneyError::DivisionByZero => write!(f, "division by zero"),
            MoneyError::CurrencyMismatch(a, b) => write!(f, "currency mismatch: {a} vs {b}"),
            MoneyError::Parse(s) => write!(f, "cannot parse money: {s}"),
            MoneyError::Unbalanced { sum } => write!(f, "entry does not balance (sum {sum})"),
            MoneyError::NoWeights => write!(f, "all weights are zero"),
        }
    }
}
impl std::error::Error for MoneyError {}
type Res<T> = Result<T, MoneyError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Round {
    HalfUp,   // ties away from zero (what most people learn in school; biased upward in magnitude)
    HalfEven, // ties to the even neighbour (banker's rounding; unbiased over many ties)
    Floor,    // toward -infinity
    Ceil,     // toward +infinity
    TowardZero,
}
const ROUNDS: [Round; 5] = [
    Round::HalfUp,
    Round::HalfEven,
    Round::Floor,
    Round::Ceil,
    Round::TowardZero,
];

fn pow10(k: u32) -> Res<i128> {
    10i128.checked_pow(k).ok_or(MoneyError::Overflow)
}

/// n / d rounded to an integer by `mode`. The one place rounding logic lives.
fn div_round(n: i128, d: i128, mode: Round) -> Res<i128> {
    if d == 0 {
        return Err(MoneyError::DivisionByZero);
    }
    let (n, d) = if d < 0 {
        (n.checked_neg().ok_or(MoneyError::Overflow)?, -d)
    } else {
        (n, d)
    };
    let q = n.div_euclid(d); // floor, because d > 0
    let r = n.rem_euclid(d); // 0 <= r < d, value = q + r/d
    if r == 0 {
        return Ok(q);
    }
    let twice = r * 2; // compare the fraction with one half without any division
    let up = match mode {
        Round::Floor => false,
        Round::Ceil => true,
        Round::TowardZero => n < 0, // for negatives floor goes away from zero, so step back up
        Round::HalfUp => twice > d || (twice == d && n > 0), // tie: away from zero = up when positive
        Round::HalfEven => twice > d || (twice == d && q.rem_euclid(2) == 1),
    };
    q.checked_add(up as i128).ok_or(MoneyError::Overflow)
}

// ============================================================ Decimal
#[derive(Clone, Copy, Debug)]
struct Decimal {
    coef: i128,
    scale: u32,
}

impl Decimal {
    fn new(coef: i128, scale: u32) -> Decimal {
        Decimal { coef, scale }
    }
    fn parse(s: &str) -> Res<Decimal> {
        let bad = || MoneyError::Parse(s.to_string());
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (int, frac) = body.split_once('.').unwrap_or((body, ""));
        if int.is_empty() && frac.is_empty()
            || !int.chars().chain(frac.chars()).all(|c| c.is_ascii_digit())
        {
            return Err(bad());
        }
        let digits = format!("{int}{frac}");
        let mag: i128 = digits.parse().map_err(|_| bad())?;
        Ok(Decimal::new(
            if neg { -mag } else { mag },
            frac.len() as u32,
        ))
    }
    fn align(a: Decimal, b: Decimal) -> Res<(i128, i128, u32)> {
        let s = a.scale.max(b.scale);
        let ca = a
            .coef
            .checked_mul(pow10(s - a.scale)?)
            .ok_or(MoneyError::Overflow)?;
        let cb = b
            .coef
            .checked_mul(pow10(s - b.scale)?)
            .ok_or(MoneyError::Overflow)?;
        Ok((ca, cb, s))
    }
    fn checked_add(self, o: Decimal) -> Res<Decimal> {
        let (a, b, s) = Decimal::align(self, o)?;
        Ok(Decimal::new(
            a.checked_add(b).ok_or(MoneyError::Overflow)?,
            s,
        ))
    }
    fn checked_sub(self, o: Decimal) -> Res<Decimal> {
        self.checked_add(Decimal::new(
            o.coef.checked_neg().ok_or(MoneyError::Overflow)?,
            o.scale,
        ))
    }
    fn checked_mul(self, o: Decimal) -> Res<Decimal> {
        Ok(Decimal::new(
            self.coef.checked_mul(o.coef).ok_or(MoneyError::Overflow)?,
            self.scale + o.scale,
        ))
    }
    /// change the scale; going down rounds with `mode`, going up is exact
    fn rescale(self, scale: u32, mode: Round) -> Res<Decimal> {
        if scale >= self.scale {
            let c = self
                .coef
                .checked_mul(pow10(scale - self.scale)?)
                .ok_or(MoneyError::Overflow)?;
            Ok(Decimal::new(c, scale))
        } else {
            Ok(Decimal::new(
                div_round(self.coef, pow10(self.scale - scale)?, mode)?,
                scale,
            ))
        }
    }
    /// quotient at an explicit scale: round(self / o) with `scale` fraction digits
    fn checked_div(self, o: Decimal, scale: u32, mode: Round) -> Res<Decimal> {
        // self/o = (c1 / 10^s1) / (c2 / 10^s2); result coef = round(c1 * 10^(scale + s2 - s1) / c2)
        let k = scale as i64 + o.scale as i64 - self.scale as i64;
        let (num, den) = if k >= 0 {
            (
                self.coef
                    .checked_mul(pow10(k as u32)?)
                    .ok_or(MoneyError::Overflow)?,
                o.coef,
            )
        } else {
            (
                self.coef,
                o.coef
                    .checked_mul(pow10((-k) as u32)?)
                    .ok_or(MoneyError::Overflow)?,
            )
        };
        Ok(Decimal::new(div_round(num, den, mode)?, scale))
    }
}
impl PartialEq for Decimal {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == std::cmp::Ordering::Equal
    }
}
impl Eq for Decimal {}
impl PartialOrd for Decimal {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Decimal {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        let (a, b, _) = Decimal::align(*self, *o).expect("comparison operands too large");
        a.cmp(&b)
    }
}
impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mag = self.coef.unsigned_abs().to_string();
        let sign = if self.coef < 0 { "-" } else { "" };
        if self.scale == 0 {
            return write!(f, "{sign}{mag}");
        }
        let padded = format!("{:0>width$}", mag, width = self.scale as usize + 1);
        let (i, fr) = padded.split_at(padded.len() - self.scale as usize);
        write!(f, "{sign}{i}.{fr}")
    }
}

// ============================================================ currency and money
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Currency {
    code: &'static str,
    exp: u32, // digits after the decimal point in the minor unit
}
const USD: Currency = Currency {
    code: "USD",
    exp: 2,
};
const EUR: Currency = Currency {
    code: "EUR",
    exp: 2,
};
const JPY: Currency = Currency {
    code: "JPY",
    exp: 0,
};
const KWD: Currency = Currency {
    code: "KWD",
    exp: 3,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Money {
    minor: i64,
    cur: Currency,
}

impl Money {
    fn new(minor: i64, cur: Currency) -> Money {
        Money { minor, cur }
    }
    fn as_decimal(self) -> Decimal {
        Decimal::new(self.minor as i128, self.cur.exp)
    }
    fn from_decimal(d: Decimal, cur: Currency, mode: Round) -> Res<Money> {
        let r = d.rescale(cur.exp, mode)?;
        Ok(Money::new(
            i64::try_from(r.coef).map_err(|_| MoneyError::Overflow)?,
            cur,
        ))
    }
    fn same(self, o: Money) -> Res<()> {
        if self.cur == o.cur {
            Ok(())
        } else {
            Err(MoneyError::CurrencyMismatch(self.cur.code, o.cur.code))
        }
    }
    fn checked_add(self, o: Money) -> Res<Money> {
        self.same(o)?;
        Ok(Money::new(
            self.minor
                .checked_add(o.minor)
                .ok_or(MoneyError::Overflow)?,
            self.cur,
        ))
    }
    fn checked_sub(self, o: Money) -> Res<Money> {
        self.same(o)?;
        Ok(Money::new(
            self.minor
                .checked_sub(o.minor)
                .ok_or(MoneyError::Overflow)?,
            self.cur,
        ))
    }
    fn checked_mul_int(self, k: i64) -> Res<Money> {
        Ok(Money::new(
            self.minor.checked_mul(k).ok_or(MoneyError::Overflow)?,
            self.cur,
        ))
    }
    /// multiply by a rate (tax, interest) and round ONCE, back to minor units
    fn apply_rate(self, rate: Decimal, mode: Round) -> Res<Money> {
        Money::from_decimal(self.as_decimal().checked_mul(rate)?, self.cur, mode)
    }
    /// convert with an exchange rate (units of `to` per unit of self's currency)
    fn convert(self, rate: Decimal, to: Currency, mode: Round) -> Res<Money> {
        Money::from_decimal(self.as_decimal().checked_mul(rate)?, to, mode)
    }
}

/// Largest remainder allocation: shares are proportional to weights and sum EXACTLY to `total`.
fn allocate(total: i64, weights: &[u64]) -> Res<Vec<i64>> {
    let w_sum: u128 = weights.iter().map(|&w| w as u128).sum();
    if w_sum == 0 {
        return Err(MoneyError::NoWeights);
    }
    let t = total.unsigned_abs() as u128;
    let mut floors = Vec::with_capacity(weights.len());
    let mut rems = Vec::with_capacity(weights.len());
    for &w in weights {
        let prod = t.checked_mul(w as u128).ok_or(MoneyError::Overflow)?;
        floors.push(prod / w_sum);
        rems.push(prod % w_sum);
    }
    let mut leftover = t - floors.iter().sum::<u128>(); // always < number of shares with a non-zero remainder
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by(|&a, &b| rems[b].cmp(&rems[a]).then(a.cmp(&b))); // biggest remainder first, lower index on ties
    for &i in &order {
        if leftover == 0 {
            break;
        }
        floors[i] += 1;
        leftover -= 1;
    }
    let sign = if total < 0 { -1i128 } else { 1 };
    Ok(floors
        .into_iter()
        .map(|f| (sign * f as i128) as i64)
        .collect())
}

// ============================================================ formatting and parsing
#[derive(Clone, Copy)]
struct Locale {
    decimal: char,
    group: char,
    symbol_first: bool,
}
const EN_US: Locale = Locale {
    decimal: '.',
    group: ',',
    symbol_first: true,
};
const DE_DE: Locale = Locale {
    decimal: ',',
    group: '.',
    symbol_first: false,
};
const FR_FR: Locale = Locale {
    decimal: ',',
    group: ' ',
    symbol_first: false,
};

fn symbol(cur: Currency) -> &'static str {
    cur.code
}

fn format_money(m: Money, loc: Locale) -> String {
    let digits = m.minor.unsigned_abs().to_string();
    let exp = m.cur.exp as usize;
    let padded = format!("{:0>w$}", digits, w = exp + 1);
    let (int, frac) = padded.split_at(padded.len() - exp);
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(loc.group);
        }
        grouped.push(ch);
    }
    let num = if exp == 0 {
        grouped
    } else {
        format!("{grouped}{}{frac}", loc.decimal)
    };
    let sign = if m.minor < 0 { "-" } else { "" };
    if loc.symbol_first {
        format!("{sign}{} {num}", symbol(m.cur))
    } else {
        format!("{sign}{num} {}", symbol(m.cur))
    }
}

fn parse_money(s: &str, loc: Locale, cur: Currency) -> Res<Money> {
    let bad = || MoneyError::Parse(s.to_string());
    let t = s.trim().replace(symbol(cur), "");
    let t = t.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r.trim()),
        None => (false, t),
    };
    let (int, frac) = match t.split_once(loc.decimal) {
        Some((i, f)) => (i, f),
        None => (t, ""),
    };
    let int: String = int.chars().filter(|&c| c != loc.group).collect();
    if int.is_empty()
        || !int.chars().all(|c| c.is_ascii_digit())
        || !frac.chars().all(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    if frac.len() > cur.exp as usize
        || (cur.exp > 0
            && frac.len() != cur.exp as usize
            && s.contains(loc.decimal)
            && frac.is_empty())
    {
        return Err(bad()); // never round silently while parsing
    }
    let frac = format!("{frac:0<w$}", w = cur.exp as usize);
    let mag: u128 = format!("{int}{frac}").parse().map_err(|_| bad())?;
    let minor = if neg {
        if mag > 1u128 << 63 {
            return Err(MoneyError::Overflow);
        }
        (-(mag as i128)) as i64
    } else {
        i64::try_from(mag).map_err(|_| MoneyError::Overflow)?
    };
    Ok(Money::new(minor, cur))
}

// ============================================================ ledger
#[derive(Clone, Debug, Default, PartialEq)]
struct Ledger {
    balances: BTreeMap<String, i64>,
    entries: u64,
}
impl Ledger {
    /// Post one entry atomically. Postings must sum to zero (debits positive, credits negative, or vice versa).
    fn post(&mut self, postings: &[(&str, i64)]) -> Res<()> {
        let sum: i128 = postings.iter().map(|p| p.1 as i128).sum();
        if sum != 0 {
            return Err(MoneyError::Unbalanced { sum });
        }
        let mut staged: BTreeMap<String, i64> = BTreeMap::new();
        for &(acct, amt) in postings {
            let cur = staged
                .get(acct)
                .or(self.balances.get(acct))
                .copied()
                .unwrap_or(0);
            staged.insert(
                acct.to_string(),
                cur.checked_add(amt).ok_or(MoneyError::Overflow)?,
            );
        }
        self.balances.extend(staged); // commit only after every posting succeeded
        self.entries += 1;
        Ok(())
    }
    fn total(&self) -> i128 {
        self.balances.values().map(|&v| v as i128).sum()
    }
}

// ============================================================ checks
fn exhaustive_rounding() {
    let mut checked = 0;
    for n in -60i128..=60 {
        for d in (-12i128..=12).filter(|&d| d != 0) {
            let x = n as f64 / d as f64;
            let (lo, hi) = (x.floor() as i128, x.ceil() as i128);
            let is_int = lo == hi;
            // exact tie test, independent of div_round: x == lo + 1/2  <=>  2n == (2lo+1) d
            let tie = !is_int && 2 * n == (2 * lo + 1) * d;
            for mode in ROUNDS {
                let want = if is_int {
                    lo
                } else {
                    match mode {
                        Round::Floor => lo,
                        Round::Ceil => hi,
                        Round::TowardZero => {
                            if x < 0.0 {
                                hi
                            } else {
                                lo
                            }
                        }
                        Round::HalfUp => {
                            if tie {
                                if x > 0.0 { hi } else { lo }
                            } else {
                                x.round() as i128
                            }
                        }
                        Round::HalfEven => {
                            if tie {
                                if lo % 2 == 0 { lo } else { hi }
                            } else {
                                x.round() as i128
                            }
                        }
                    }
                };
                assert_eq!(div_round(n, d, mode).unwrap(), want, "{n}/{d} {mode:?}");
                checked += 1;
            }
        }
    }
    assert_eq!(
        div_round(1, 0, Round::HalfUp),
        Err(MoneyError::DivisionByZero)
    );
    assert_eq!(
        div_round(i128::MIN, -1, Round::Floor),
        Err(MoneyError::Overflow)
    );
    println!(
        "rounding: {checked} (n, d, mode) cases of div_round match the float-floor + exact-tie reference"
    );
    // table for the classic ties
    println!("   x    half-up  half-even  floor  ceil  toward0");
    for twice in [-5i128, -3, -1, 1, 3, 5] {
        let r: Vec<i128> = ROUNDS
            .iter()
            .map(|&m| div_round(twice, 2, m).unwrap())
            .collect();
        println!(
            "{:>5.1}  {:>7}  {:>9}  {:>5}  {:>4}  {:>7}",
            twice as f64 / 2.0,
            r[0],
            r[1],
            r[2],
            r[3],
            r[4]
        );
    }
}

fn tie_bias() {
    // 1000 amounts x.5 minor units: half-up always rounds away, half-even alternates, so its errors cancel
    let (mut exact_twice, mut up, mut even) = (0i128, 0i128, 0i128);
    for x in 0..1000i128 {
        let twice = 2 * x + 1; // value (2x+1)/2 = x + 0.5
        exact_twice += twice;
        up += div_round(twice, 2, Round::HalfUp).unwrap();
        even += div_round(twice, 2, Round::HalfEven).unwrap();
    }
    println!(
        "1000 ties: exact total {} minor units, half-up {up} (bias +{}), half-even {even} (bias {})",
        exact_twice / 2,
        up - exact_twice / 2,
        even - exact_twice / 2
    );
    assert_eq!(up - exact_twice / 2, 500);
    assert_eq!(even, exact_twice / 2);
}

fn check_allocation(rng: &mut Rng) {
    let (mut naive_wrong, mut cases) = (0, 0);
    for _ in 0..3000 {
        let n = 1 + (rng.next() % 6) as usize;
        let mut weights: Vec<u64> = (0..n).map(|_| rng.next() % 50).collect();
        if weights.iter().all(|&w| w == 0) {
            weights[0] = 1;
        }
        let total = rng.range(-100_000, 100_000);
        let shares = allocate(total, &weights).unwrap();
        let w_sum: i128 = weights.iter().map(|&w| w as i128).sum();
        assert_eq!(
            shares.iter().map(|&s| s as i128).sum::<i128>(),
            total as i128,
            "shares must sum to the total"
        );
        for (i, &s) in shares.iter().enumerate() {
            let exact_num = total as i128 * weights[i] as i128; // share = exact_num / w_sum
            let (fl, ce) = (
                exact_num.div_euclid(w_sum),
                -((-exact_num).div_euclid(w_sum)),
            );
            assert!(
                s as i128 == fl || s as i128 == ce,
                "share {s} must be floor or ceil of exact {exact_num}/{w_sum}"
            );
            if weights[i] == 0 {
                assert_eq!(s, 0, "zero weight gets nothing");
            }
        }
        let neg = allocate(-total, &weights).unwrap();
        assert!(
            neg.iter().zip(&shares).all(|(a, b)| *a == -*b),
            "sign symmetric"
        );
        // compare with naive "round every share" which does NOT preserve the sum
        let naive: i64 = weights
            .iter()
            .map(|&w| div_round(total as i128 * w as i128, w_sum, Round::HalfUp).unwrap() as i64)
            .sum();
        cases += 1;
        naive_wrong += (naive != total) as i32;
    }
    assert_eq!(allocate(1, &[0, 0]), Err(MoneyError::NoWeights));
    assert_eq!(allocate(100, &[1, 1, 1]).unwrap(), vec![34, 33, 33]);
    assert_eq!(allocate(5, &[1, 1]).unwrap(), vec![3, 2]);
    println!(
        "allocate: 3000 random splits sum exactly; naive rounding of each share missed the total in {naive_wrong}/{cases} cases"
    );
    assert!(naive_wrong > 100);
}

fn check_decimal(rng: &mut Rng) {
    let (mut assoc_exact, mut assoc_rounded_diff) = (0, 0);
    for _ in 0..3000 {
        let mk = |rng: &mut Rng| {
            Decimal::new(rng.range(-99_999, 99_999) as i128, (rng.next() % 5) as u32)
        };
        let (a, b, c) = (mk(rng), mk(rng), mk(rng));
        // exact decimal arithmetic IS associative and commutative
        assert_eq!(
            a.checked_add(b).unwrap().checked_add(c).unwrap(),
            a.checked_add(b.checked_add(c).unwrap()).unwrap()
        );
        assert_eq!(
            a.checked_mul(b).unwrap().checked_mul(c).unwrap(),
            a.checked_mul(b.checked_mul(c).unwrap()).unwrap()
        );
        assert_eq!(a.checked_add(b).unwrap(), b.checked_add(a).unwrap());
        assert_eq!(a.checked_sub(a).unwrap(), Decimal::new(0, 0));
        assoc_exact += 1;
        // parse(display(x)) round trip, including scale
        let s = a.to_string();
        let back = Decimal::parse(&s).unwrap();
        assert!(
            back == a && back.scale == a.scale && back.to_string() == s,
            "round trip {s}"
        );
        // rounding after each multiplication is NOT associative
        let r = |x: Decimal| x.rescale(2, Round::HalfUp).unwrap();
        let left = r(r(a.checked_mul(b).unwrap()).checked_mul(c).unwrap());
        let right = r(a.checked_mul(r(b.checked_mul(c).unwrap())).unwrap());
        assoc_rounded_diff += (left != right) as i32;
        // division then multiplication back stays within one unit of the target scale
        if b.coef != 0 {
            let q = a.checked_div(b, 6, Round::HalfEven).unwrap();
            let back = q.checked_mul(b).unwrap();
            let err = back.checked_sub(a).unwrap();
            let bound = Decimal::new(b.coef.abs(), b.scale + 6); // |b| * 10^-6 (half-ulp bound with slack)
            assert!(
                Decimal::new(err.coef.abs(), err.scale) <= bound,
                "division error bound"
            );
        }
    }
    println!(
        "decimal: {assoc_exact} random triples: exact add/mul associative and display/parse round trip; {assoc_rounded_diff} triples gave different results when rounding after each multiply"
    );
    assert!(assoc_rounded_diff > 0);
    assert_eq!(Decimal::parse("-0.05").unwrap().to_string(), "-0.05");
    assert!(
        Decimal::parse("1.2.3").is_err()
            && Decimal::parse("").is_err()
            && Decimal::parse("abc").is_err()
    );
    assert_eq!(
        Decimal::parse("2.50").unwrap(),
        Decimal::parse("2.5").unwrap()
    );
    assert_eq!(
        Decimal::new(1, 0)
            .checked_div(Decimal::new(3, 0), 4, Round::HalfUp)
            .unwrap()
            .to_string(),
        "0.3333"
    );
    assert_eq!(
        Decimal::new(2, 0)
            .checked_div(Decimal::new(3, 0), 4, Round::HalfUp)
            .unwrap()
            .to_string(),
        "0.6667"
    );
    assert_eq!(
        Decimal::new(1, 0).checked_div(Decimal::new(0, 0), 2, Round::HalfUp),
        Err(MoneyError::DivisionByZero)
    );
    // overflow is an error, not a wrap
    assert_eq!(
        Decimal::new(i128::MAX, 0).checked_add(Decimal::new(1, 0)),
        Err(MoneyError::Overflow)
    );
    assert_eq!(
        Money::new(i64::MAX, USD).checked_add(Money::new(1, USD)),
        Err(MoneyError::Overflow)
    );
    assert_eq!(
        Money::new(i64::MAX, USD).checked_mul_int(2),
        Err(MoneyError::Overflow)
    );
    assert_eq!(
        Money::new(1, USD).checked_add(Money::new(1, EUR)),
        Err(MoneyError::CurrencyMismatch("USD", "EUR"))
    );
}

fn check_pitfalls(rng: &mut Rng) {
    // 1. tax: round each line, or round the invoice total?
    let rate = Decimal::parse("0.0825").unwrap();
    let (mut differ, mut max_gap, mut baskets) = (0, 0i64, 0);
    for _ in 0..3000 {
        let lines = 1 + (rng.next() % 8) as usize;
        let items: Vec<Money> = (0..lines)
            .map(|_| Money::new(rng.range(1, 5000) * rng.range(1, 5), USD))
            .collect();
        let per_line: i64 = items
            .iter()
            .map(|m| m.apply_rate(rate, Round::HalfUp).unwrap().minor)
            .sum();
        let subtotal = items
            .iter()
            .fold(Money::new(0, USD), |a, &m| a.checked_add(m).unwrap());
        let per_invoice = subtotal.apply_rate(rate, Round::HalfUp).unwrap().minor;
        let gap = (per_line - per_invoice).abs();
        // every rounding is within half a cent, so the totals can differ by at most `lines` cents
        assert!(gap <= lines as i64, "gap {gap} lines {lines}");
        differ += (gap != 0) as i32;
        max_gap = max_gap.max(gap);
        baskets += 1;
    }
    println!(
        "tax 8.25%: per-line vs per-invoice rounding differ in {differ}/{baskets} random baskets (max gap {max_gap} cents): pick one by contract/law and write it down"
    );
    assert!(differ > 100);
    // the textbook case
    let price = Money::new(1999, USD);
    let tax = price.apply_rate(rate, Round::HalfUp).unwrap();
    assert_eq!(tax.minor, 165); // 164.9175 cents

    // 2. compound interest: round each month (what a bank statement shows) vs one rounding at the end
    let principal = Money::new(1_000_00, USD);
    let monthly = Decimal::parse("0.05")
        .unwrap()
        .checked_div(Decimal::new(12, 0), 12, Round::HalfEven)
        .unwrap();
    let mut bal = principal;
    let mut hp = principal.as_decimal().rescale(12, Round::HalfEven).unwrap();
    for _ in 0..12 {
        bal = bal
            .checked_add(bal.apply_rate(monthly, Round::HalfEven).unwrap())
            .unwrap();
        let growth = hp
            .checked_mul(monthly)
            .unwrap()
            .rescale(12, Round::HalfEven)
            .unwrap();
        hp = hp.checked_add(growth).unwrap();
    }
    let end = Money::from_decimal(hp, USD, Round::HalfEven).unwrap();
    let float = 1000.0 * (1.0 + 0.05f64 / 12.0).powi(12);
    println!(
        "interest: 1000.00 at 5%/yr, 12 monthly periods: rounded each month {}, high precision {}, f64 {float:.6}",
        bal.as_decimal(),
        end.as_decimal()
    );
    assert!((bal.minor - end.minor).abs() <= 12);

    // 3. currency conversion with different exponents
    let yen = Money::new(1000, JPY);
    let usd = yen
        .convert(Decimal::parse("0.0067").unwrap(), USD, Round::HalfEven)
        .unwrap();
    assert_eq!(usd.minor, 670); // 6.70 USD
    let back = usd
        .convert(Decimal::parse("149.25").unwrap(), JPY, Round::HalfEven)
        .unwrap();
    println!(
        "convert: 1000 JPY -> {} USD -> {} JPY (round trip loses {} yen)",
        usd.as_decimal(),
        back.minor,
        1000 - back.minor
    );
    let kwd = Money::new(12_345, KWD); // 12.345 KWD
    assert_eq!(kwd.as_decimal().to_string(), "12.345");
    assert_eq!(
        kwd.convert(Decimal::parse("3.25").unwrap(), USD, Round::HalfUp)
            .unwrap()
            .minor,
        4012
    ); // 40.12125 -> 40.12

    // 4. float comparison: where f64 goes wrong
    let mut acc = 0.0f64;
    for _ in 0..1_000_000 {
        acc += 0.01;
    }
    let mut cents = 0i64;
    for _ in 0..1_000_000 {
        cents += 1;
    }
    println!(
        "f64: 0.01 added 1e6 times = {acc:.9} (drift {:.3e}); integer cents = {cents} (exact)",
        acc - 10_000.0
    );
    assert!(acc != 10_000.0 && cents == 1_000_000);
    assert_eq!(format!("{:.2}", 1.005f64), "1.00"); // the double nearest 1.005 is below it
    assert_eq!(
        Decimal::parse("1.005")
            .unwrap()
            .rescale(2, Round::HalfUp)
            .unwrap()
            .to_string(),
        "1.01"
    );
    let mut f64_wrong = 0;
    for k in 0..5000i128 {
        let dec = Decimal::new(k, 3).rescale(2, Round::HalfUp).unwrap().coef;
        let via_f64 = ((k as f64 / 1000.0) * 100.0).round() as i128;
        f64_wrong += (dec != via_f64) as i32;
    }
    println!(
        "f64 round-to-cents of x.xxx disagrees with decimal half-up on {f64_wrong}/5000 three-digit prices"
    );
    assert!(f64_wrong > 0);
}

fn check_formatting(rng: &mut Rng) {
    assert_eq!(
        format_money(Money::new(123_456, USD), EN_US),
        "USD 1,234.56"
    );
    assert_eq!(format_money(Money::new(-5, USD), EN_US), "-USD 0.05");
    assert_eq!(
        format_money(Money::new(123_456, EUR), DE_DE),
        "1.234,56 EUR"
    );
    assert_eq!(
        format_money(Money::new(1_234_567, JPY), EN_US),
        "JPY 1,234,567"
    );
    assert_eq!(
        format_money(Money::new(1_234_567, KWD), FR_FR),
        "1 234,567 KWD"
    );
    assert_eq!(parse_money("1,234.56", EN_US, USD).unwrap().minor, 123_456);
    assert!(
        parse_money("1.234", EN_US, USD).is_err(),
        "3 decimals for a 2-decimal currency must be rejected"
    );
    assert!(parse_money("12x", EN_US, USD).is_err() && parse_money("", EN_US, USD).is_err());
    assert_eq!(
        parse_money("-9223372036854775808", EN_US, JPY)
            .unwrap()
            .minor,
        i64::MIN
    );
    assert_eq!(
        parse_money("9223372036854775808", EN_US, JPY),
        Err(MoneyError::Overflow)
    );
    let curs = [USD, EUR, JPY, KWD];
    let locs = [EN_US, DE_DE, FR_FR];
    for _ in 0..3000 {
        let cur = curs[(rng.next() % 4) as usize];
        let loc = locs[(rng.next() % 3) as usize];
        let minor = match rng.next() % 4 {
            0 => rng.range(-1000, 1000),
            1 => rng.next() as i64,
            2 => i64::MIN + (rng.next() % 3) as i64,
            _ => i64::MAX - (rng.next() % 3) as i64,
        };
        let m = Money::new(minor, cur);
        let text = format_money(m, loc);
        assert_eq!(
            parse_money(&text, loc, cur).unwrap(),
            m,
            "round trip {text}"
        );
    }
    println!(
        "format/parse: 3000 random amounts (including i64::MIN/MAX) x 4 currencies x 3 locales round-trip"
    );
}

fn check_ledger(rng: &mut Rng) {
    let mut ledger = Ledger::default();
    let accounts = [
        "cash",
        "revenue",
        "fees",
        "tax",
        "partner_a",
        "partner_b",
        "customer",
    ];
    let (mut ok, mut rejected_unbalanced, mut rejected_overflow) = (0, 0, 0);
    for step in 0..3000 {
        let before = ledger.clone();
        match rng.next() % 6 {
            0 | 1 => {
                // customer pays; fee split between three accounts by weights
                let amount = rng.range(1, 1_000_000);
                let weights = [rng.next() % 10, rng.next() % 10, 1 + rng.next() % 10];
                let shares = allocate(amount, &weights).unwrap();
                let postings = vec![
                    ("customer", amount),
                    ("fees", -shares[0]),
                    ("tax", -shares[1]),
                    ("revenue", -shares[2]),
                ];
                ledger.post(&postings).unwrap();
                ok += 1;
            }
            2 => {
                let amount = rng.range(-5000, 5000);
                ledger
                    .post(&[
                        (accounts[(rng.next() % 7) as usize], amount),
                        ("cash", -amount),
                    ])
                    .unwrap();
                ok += 1;
            }
            3 => {
                // failure injection: off-by-one-cent entry must be rejected and change nothing
                let amount = rng.range(1, 10_000);
                let err = ledger
                    .post(&[("cash", amount), ("revenue", -amount + 1)])
                    .unwrap_err();
                assert_eq!(err, MoneyError::Unbalanced { sum: 1 });
                assert_eq!(ledger, before, "rejected entry changed the ledger");
                rejected_unbalanced += 1;
            }
            4 => {
                // overflow injection: balanced entry whose second posting overflows must be atomic
                ledger
                    .post(&[("big", i64::MAX - 5), ("cash", -(i64::MAX - 5))])
                    .ok();
                let b = ledger.clone();
                let r = ledger.post(&[("cash", 3), ("big", 10), ("revenue", -13)]);
                if r == Err(MoneyError::Overflow) {
                    assert_eq!(ledger, b, "failed entry must leave the ledger untouched");
                    rejected_overflow += 1;
                }
                ledger
                    .post(&[
                        ("big", -(ledger.balances["big"])),
                        ("cash", ledger.balances["big"]),
                    ])
                    .unwrap();
            }
            _ => {
                let parts = allocate(rng.range(1, 100_000), &[3, 2, 1, 1]).unwrap();
                let total: i64 = parts.iter().sum();
                ledger
                    .post(&[
                        ("customer", total),
                        ("partner_a", -parts[0]),
                        ("partner_b", -parts[1]),
                        ("tax", -parts[2] - parts[3]),
                    ])
                    .unwrap();
                ok += 1;
            }
        }
        assert_eq!(
            ledger.total(),
            0,
            "sum of all balances is zero at step {step}"
        );
    }
    println!(
        "ledger: 3000 random operations: {ok} posted, {rejected_unbalanced} unbalanced entries rejected, {rejected_overflow} overflows rejected atomically; sum of balances always 0"
    );
    assert!(rejected_unbalanced > 100 && rejected_overflow > 10);
}

fn main() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    exhaustive_rounding();
    tie_bias();
    check_allocation(&mut rng);
    check_decimal(&mut rng);
    check_pitfalls(&mut rng);
    check_formatting(&mut rng);
    check_ledger(&mut rng);
    println!("all money checks passed");
}
