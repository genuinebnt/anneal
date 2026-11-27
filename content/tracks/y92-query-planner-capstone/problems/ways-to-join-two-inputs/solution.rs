use std::collections::{BTreeMap, HashMap, VecDeque};

// ---- column summaries ----

/// A summary cuts a column's sorted values into this many slices of (nearly) equal size.
pub const SLICES: usize = 32;
/// At most this many values are listed with their own counts.
pub const COMMON: usize = 8;

/// All the planner remembers about one column.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnStats {
    /// All rows, those with a missing value included.
    pub rows: usize,
    pub missing: usize,
    /// How many different values are present.
    pub distinct: usize,
    /// `SLICES + 1` values that bound the slices; empty if no value is present.
    pub cuts: Vec<i64>,
    /// Unusually frequent values with their exact counts, most frequent first.
    pub common: Vec<(i64, usize)>,
}

pub fn summarise(values: &[Option<i64>]) -> ColumnStats {
    let mut sorted: Vec<i64> = values.iter().flatten().copied().collect();
    sorted.sort_unstable();
    let present = sorted.len();
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for &v in &sorted {
        match counts.last_mut() {
            Some((value, count)) if *value == v => *count += 1,
            _ => counts.push((v, 1)),
        }
    }
    let distinct = counts.len();
    let cuts = if present == 0 {
        Vec::new()
    } else {
        (0..=SLICES)
            .map(|i| sorted[i * (present - 1) / SLICES])
            .collect()
    };
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    counts.truncate(COMMON);
    // more than 1.25 times the average count, in whole numbers
    counts.retain(|&(_, count)| 4 * count * distinct > 5 * present);
    ColumnStats {
        rows: values.len(),
        missing: values.len() - present,
        distinct,
        cuts,
        common: counts,
    }
}

impl ColumnStats {
    pub fn present(&self) -> usize {
        self.rows - self.missing
    }

    /// The estimated number of rows whose value is `value`.
    pub fn equal_to(&self, value: i64) -> f64 {
        let (Some(&low), Some(&high)) = (self.cuts.first(), self.cuts.last()) else {
            return 0.0;
        };
        if value < low || value > high {
            return 0.0;
        }
        if let Some(&(_, count)) = self.common.iter().find(|c| c.0 == value) {
            return count as f64;
        }
        let listed: usize = self.common.iter().map(|c| c.1).sum();
        match self.distinct - self.common.len() {
            0 => 0.0,
            others => (self.present() - listed) as f64 / others as f64,
        }
    }

    /// The estimated number of present values that are at most `value`.
    fn at_most(&self, value: i64) -> f64 {
        let (Some(&low), Some(&high)) = (self.cuts.first(), self.cuts.last()) else {
            return 0.0;
        };
        if value < low {
            return 0.0;
        }
        if value >= high {
            return self.present() as f64;
        }
        let slice = self.cuts[..SLICES].partition_point(|&c| c <= value) - 1;
        let (a, b) = (self.cuts[slice] as i128, self.cuts[slice + 1] as i128);
        let inside = (value as i128 - a + 1) as f64 / (b - a + 1) as f64;
        (slice as f64 + inside) / SLICES as f64 * self.present() as f64
    }

    /// The estimated number of rows whose value is in `low..=high`.
    pub fn between(&self, low: i64, high: i64) -> f64 {
        if low > high {
            return 0.0;
        }
        if low == high {
            return self.equal_to(low);
        }
        let below = low.checked_sub(1).map_or(0.0, |v| self.at_most(v));
        (self.at_most(high) - below).max(0.0)
    }
}

// ---- conditions and how much they keep ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Column {
    /// The table's number in the catalog.
    pub table: usize,
    /// The column's number in that table.
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cond {
    /// `column <cmp> number`
    Compare(Column, Cmp, i64),
    /// `column = column`, for two different columns
    Same(Column, Column),
    /// `column in (numbers)`
    OneOf(Column, Vec<i64>),
    /// Every part holds. An empty list always holds.
    All(Vec<Cond>),
    /// At least one part holds. An empty list never holds.
    Any(Vec<Cond>),
}

#[derive(Debug, Clone)]
pub struct Table {
    pub name: String,
    pub column_names: Vec<String>,
    pub rows: usize,
    /// One summary per column.
    pub stats: Vec<ColumnStats>,
    /// The columns that have an index.
    pub indexed: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub tables: Vec<Table>,
}

/// The inclusive range of numbers that `x <cmp> value` allows; `low > high` if there are none.
/// `None` for `Ne`, which is not one range.
pub fn range_of(cmp: Cmp, value: i64) -> Option<(i64, i64)> {
    const NOTHING: (i64, i64) = (1, 0);
    Some(match cmp {
        Cmp::Eq => (value, value),
        Cmp::Lt => value.checked_sub(1).map_or(NOTHING, |h| (i64::MIN, h)),
        Cmp::Le => (i64::MIN, value),
        Cmp::Gt => value.checked_add(1).map_or(NOTHING, |l| (l, i64::MAX)),
        Cmp::Ge => (value, i64::MAX),
        Cmp::Ne => return None,
    })
}

/// The estimated fraction of rows for which the condition holds, from 0 to 1.
pub fn selectivity(condition: &Cond, catalog: &Catalog) -> f64 {
    let stats = |c: &Column| &catalog.tables[c.table].stats[c.column];
    let share = |c: &Column, matching: f64| match catalog.tables[c.table].rows {
        0 => 0.0,
        rows => (matching / rows as f64).clamp(0.0, 1.0),
    };
    match condition {
        Cond::Compare(c, cmp, value) => match cmp {
            Cmp::Eq => share(c, stats(c).equal_to(*value)),
            Cmp::Ne => share(c, stats(c).present() as f64 - stats(c).equal_to(*value)),
            _ => {
                let (low, high) = range_of(*cmp, *value).expect("an ordering");
                share(c, stats(c).between(low, high))
            }
        },
        Cond::Same(a, b) => match (stats(a).distinct, stats(b).distinct) {
            (0, _) | (_, 0) => 0.0,
            (x, y) => 1.0 / x.max(y) as f64,
        },
        Cond::OneOf(c, values) => {
            let mut values = values.clone();
            values.sort_unstable();
            values.dedup();
            let sum: f64 = values.iter().map(|&v| share(c, stats(c).equal_to(v))).sum();
            sum.min(1.0)
        }
        Cond::All(_) => {
            let mut parts = Vec::new();
            conjuncts(condition, &mut parts);
            let mut ranges: BTreeMap<Column, (i64, i64)> = BTreeMap::new();
            let mut kept = 1.0;
            for part in &parts {
                match part {
                    Cond::Compare(c, cmp @ (Cmp::Lt | Cmp::Le | Cmp::Gt | Cmp::Ge), value) => {
                        let (low, high) = range_of(*cmp, *value).expect("an ordering");
                        let range = ranges.entry(*c).or_insert((i64::MIN, i64::MAX));
                        *range = (range.0.max(low), range.1.min(high));
                    }
                    other => kept *= selectivity(other, catalog),
                }
            }
            for (c, (low, high)) in &ranges {
                kept *= share(c, stats(c).between(*low, *high));
            }
            kept
        }
        Cond::Any(parts) => {
            let missed: f64 = parts
                .iter()
                .map(|p| 1.0 - selectivity(p, catalog))
                .product();
            1.0 - missed
        }
    }
}

/// The parts that must all hold: nested `All` lists are opened up, everything else is one part.
pub fn conjuncts(condition: &Cond, out: &mut Vec<Cond>) {
    match condition {
        Cond::All(parts) => parts.iter().for_each(|p| conjuncts(p, out)),
        other => out.push(other.clone()),
    }
}

/// How far an estimate is off, as a factor of at least 1. Both numbers count as at least 1 row.
pub fn q_error(estimate: f64, actual: f64) -> f64 {
    let (e, a) = (estimate.max(1.0), actual.max(1.0));
    (e / a).max(a / e)
}

// ---- logical plans ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Logical {
    Scan(usize),
    /// Keep the rows of the input for which the condition holds.
    Filter(Cond, Box<Logical>),
    /// Every row of the left input with every row of the right input.
    Cross(Box<Logical>, Box<Logical>),
    /// The pairs of rows for which the condition holds.
    Join(Cond, Box<Logical>, Box<Logical>),
}

/// The tables a condition reads, as bits (table numbers are below 64).
pub fn tables_of(condition: &Cond) -> u64 {
    match condition {
        Cond::Compare(c, ..) | Cond::OneOf(c, _) => 1 << c.table,
        Cond::Same(a, b) => (1 << a.table) | (1 << b.table),
        Cond::All(parts) | Cond::Any(parts) => parts.iter().fold(0, |m, p| m | tables_of(p)),
    }
}

/// One condition that stands for a non-empty list of parts.
fn combine(mut parts: Vec<Cond>) -> Cond {
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        Cond::All(parts)
    }
}

fn scanned(plan: &Logical) -> u64 {
    match plan {
        Logical::Scan(t) => 1 << t,
        Logical::Filter(_, input) => scanned(input),
        Logical::Cross(a, b) | Logical::Join(_, a, b) => scanned(a) | scanned(b),
    }
}

/// Rebuild `plan` without its old conditions, placing `waiting` and the plan's own conditions as low as possible.
fn place(plan: &Logical, mut waiting: Vec<Cond>) -> Logical {
    match plan {
        Logical::Filter(condition, input) => {
            conjuncts(condition, &mut waiting);
            place(input, waiting)
        }
        Logical::Scan(t) => match waiting.is_empty() {
            true => Logical::Scan(*t),
            false => Logical::Filter(combine(waiting), Box::new(Logical::Scan(*t))),
        },
        Logical::Cross(a, b) | Logical::Join(_, a, b) => {
            if let Logical::Join(condition, ..) = plan {
                conjuncts(condition, &mut waiting);
            }
            let (left_tables, right_tables) = (scanned(a), scanned(b));
            let (mut left, mut right, mut here) = (Vec::new(), Vec::new(), Vec::new());
            for part in waiting {
                let needs = tables_of(&part);
                if needs & !left_tables == 0 {
                    left.push(part);
                } else if needs & !right_tables == 0 {
                    right.push(part);
                } else {
                    here.push(part);
                }
            }
            let (a, b) = (Box::new(place(a, left)), Box::new(place(b, right)));
            match here.is_empty() {
                true => Logical::Cross(a, b),
                false => Logical::Join(combine(here), a, b),
            }
        }
    }
}

/// The same question, with every condition applied as early as possible.
pub fn push_down(plan: &Logical) -> Logical {
    place(plan, Vec::new())
}

// ---- physical plans and how they run ----

/// One value per column of the plan's schema; `None` is a missing value.
pub type Row = Vec<Option<i64>>;

/// Where rows come from. Each call of `row` reads one row.
pub trait Store {
    fn width(&self, table: usize) -> usize;
    fn row_count(&self, table: usize) -> usize;
    fn row(&self, table: usize, number: usize) -> Row;
    /// `(value, row number)` pairs sorted by value, then row number. Rows with a missing value are not in it.
    fn index(&self, table: usize, column: usize) -> &[(i64, usize)];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// All rows of a table in row-number order. No inputs.
    Scan(usize),
    /// The rows whose value in `column` is in `low..=high`, in index order. No inputs.
    IndexScan {
        table: usize,
        column: usize,
        low: i64,
        high: i64,
    },
    /// One input.
    Filter(Cond),
    /// One input.
    Project(Vec<Column>),
    /// Two inputs, outer then inner. `None` pairs everything.
    NestedLoop(Option<Cond>),
    /// Two inputs, probe then build: pairs with equal keys for which `rest` also holds.
    HashJoin {
        left_key: Column,
        right_key: Column,
        rest: Option<Cond>,
    },
    /// The same pairs, from two inputs that are each sorted by their key, missing keys first.
    MergeJoin {
        left_key: Column,
        right_key: Column,
        rest: Option<Cond>,
    },
    /// One input, sorted by a column, missing values first.
    Sort(Column),
}

/// A node of a physical plan. `rows`, `cost` and `order` are the planner's notes; running a plan ignores them.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub node: Node,
    pub inputs: Vec<Plan>,
    /// Estimated rows this node produces.
    pub rows: f64,
    /// Estimated cost of this node and everything under it.
    pub cost: f64,
    /// The column the output is known to be sorted by.
    pub order: Option<Column>,
}

impl Plan {
    pub fn new(node: Node, inputs: Vec<Plan>) -> Plan {
        Plan {
            node,
            inputs,
            rows: 0.0,
            cost: 0.0,
            order: None,
        }
    }
}

fn table_columns(table: usize, width: usize) -> Vec<Column> {
    (0..width).map(|column| Column { table, column }).collect()
}

/// The columns of the rows a plan produces, in order.
pub fn schema(plan: &Plan, store: &dyn Store) -> Vec<Column> {
    match &plan.node {
        Node::Scan(table) | Node::IndexScan { table, .. } => {
            table_columns(*table, store.width(*table))
        }
        Node::Filter(_) | Node::Sort(_) => schema(&plan.inputs[0], store),
        Node::Project(columns) => columns.clone(),
        Node::NestedLoop(_) | Node::HashJoin { .. } | Node::MergeJoin { .. } => {
            both_schemas(plan, store)
        }
    }
}

fn both_schemas(plan: &Plan, store: &dyn Store) -> Vec<Column> {
    let mut columns = schema(&plan.inputs[0], store);
    columns.extend(schema(&plan.inputs[1], store));
    columns
}

fn position(schema: &[Column], column: Column) -> usize {
    schema
        .iter()
        .position(|c| *c == column)
        .expect("the column is in the schema")
}

/// Whether the condition holds for a row. A missing value satisfies no comparison.
pub fn holds(condition: &Cond, row: &[Option<i64>], schema: &[Column]) -> bool {
    let value = |c: &Column| row[position(schema, *c)];
    match condition {
        Cond::Compare(c, cmp, wanted) => value(c).is_some_and(|v| match cmp {
            Cmp::Eq => v == *wanted,
            Cmp::Ne => v != *wanted,
            Cmp::Lt => v < *wanted,
            Cmp::Le => v <= *wanted,
            Cmp::Gt => v > *wanted,
            Cmp::Ge => v >= *wanted,
        }),
        Cond::Same(a, b) => value(a).is_some() && value(a) == value(b),
        Cond::OneOf(c, values) => value(c).is_some_and(|v| values.contains(&v)),
        Cond::All(parts) => parts.iter().all(|p| holds(p, row, schema)),
        Cond::Any(parts) => parts.iter().any(|p| holds(p, row, schema)),
    }
}

/// A running plan: each call hands over the next row. After `None`, it keeps returning `None`.
pub trait Operator {
    fn next(&mut self) -> Option<Row>;
}

pub type Running<'a> = Box<dyn Operator + 'a>;

/// Any iterator of rows is an operator.
struct Rows<I>(I);

impl<I: Iterator<Item = Row>> Operator for Rows<I> {
    fn next(&mut self) -> Option<Row> {
        self.0.next()
    }
}

struct Filtered<'a> {
    input: Running<'a>,
    condition: &'a Cond,
    schema: Vec<Column>,
}

impl Operator for Filtered<'_> {
    fn next(&mut self) -> Option<Row> {
        loop {
            let row = self.input.next()?;
            if holds(self.condition, &row, &self.schema) {
                return Some(row);
            }
        }
    }
}

fn drain(mut input: Running<'_>) -> Vec<Row> {
    std::iter::from_fn(|| input.next()).collect()
}

struct NestedLoop<'a> {
    outer: Running<'a>,
    /// Taken the first time the inner rows are needed.
    unread: Option<Running<'a>>,
    inner: Vec<Row>,
    current: Option<Row>,
    at: usize,
    condition: Option<&'a Cond>,
    schema: Vec<Column>,
}

impl Operator for NestedLoop<'_> {
    fn next(&mut self) -> Option<Row> {
        if let Some(input) = self.unread.take() {
            self.inner = drain(input);
        }
        loop {
            if let Some(left) = &self.current {
                while self.at < self.inner.len() {
                    let mut row = left.clone();
                    row.extend_from_slice(&self.inner[self.at]);
                    self.at += 1;
                    if self.condition.is_none_or(|c| holds(c, &row, &self.schema)) {
                        return Some(row);
                    }
                }
            }
            self.current = Some(self.outer.next()?);
            self.at = 0;
        }
    }
}

/// A glued row, if the extra condition lets it through.
fn glue(
    left: &[Option<i64>],
    right: &[Option<i64>],
    rest: Option<&Cond>,
    schema: &[Column],
) -> Option<Row> {
    let mut row = left.to_vec();
    row.extend_from_slice(right);
    rest.is_none_or(|c| holds(c, &row, schema)).then_some(row)
}

struct HashJoin<'a> {
    probe: Running<'a>,
    /// Taken the first time the build rows are needed.
    unread: Option<Running<'a>>,
    table: HashMap<i64, Vec<Row>>,
    pending: VecDeque<Row>,
    keys: (usize, usize),
    rest: Option<&'a Cond>,
    schema: Vec<Column>,
}

impl Operator for HashJoin<'_> {
    fn next(&mut self) -> Option<Row> {
        if let Some(mut build) = self.unread.take() {
            while let Some(row) = build.next() {
                if let Some(key) = row[self.keys.1] {
                    self.table.entry(key).or_default().push(row);
                }
            }
        }
        loop {
            if let Some(row) = self.pending.pop_front() {
                return Some(row);
            }
            let left = self.probe.next()?;
            let Some(matches) = left[self.keys.0].and_then(|key| self.table.get(&key)) else {
                continue;
            };
            for right in matches {
                self.pending
                    .extend(glue(&left, right, self.rest, &self.schema));
            }
        }
    }
}

struct MergeJoin<'a> {
    left: Running<'a>,
    right: Running<'a>,
    /// A right row that has been read and belongs to a later key.
    ahead: Option<Row>,
    /// The right rows with the key `group_key`.
    group: Vec<Row>,
    group_key: Option<i64>,
    current: Option<Row>,
    at: usize,
    keys: (usize, usize),
    rest: Option<&'a Cond>,
    schema: Vec<Column>,
}

impl MergeJoin<'_> {
    fn load_group(&mut self, wanted: i64) {
        self.group.clear();
        self.group_key = Some(wanted);
        while let Some(row) = self.ahead.take().or_else(|| self.right.next()) {
            match row[self.keys.1] {
                Some(key) if key == wanted => self.group.push(row),
                Some(key) if key > wanted => {
                    self.ahead = Some(row);
                    break;
                }
                _ => {}
            }
        }
    }
}

impl Operator for MergeJoin<'_> {
    fn next(&mut self) -> Option<Row> {
        loop {
            if let Some(left) = &self.current {
                while self.at < self.group.len() {
                    let right = &self.group[self.at];
                    self.at += 1;
                    if let Some(row) = glue(left, right, self.rest, &self.schema) {
                        return Some(row);
                    }
                }
                self.current = None;
            }
            let left = self.left.next()?;
            let Some(key) = left[self.keys.0] else {
                continue;
            };
            if self.group_key != Some(key) {
                self.load_group(key);
            }
            self.current = Some(left);
            self.at = 0;
        }
    }
}

fn key_positions(plan: &Plan, store: &dyn Store, left: Column, right: Column) -> (usize, usize) {
    (
        position(&schema(&plan.inputs[0], store), left),
        position(&schema(&plan.inputs[1], store), right),
    )
}

/// Start a plan. Nothing is read until the first call of `next`.
pub fn open<'a>(plan: &'a Plan, store: &'a dyn Store) -> Running<'a> {
    let input = |i: usize| open(&plan.inputs[i], store);
    match &plan.node {
        Node::Scan(table) => {
            let table = *table;
            Box::new(Rows(
                (0..store.row_count(table)).map(move |i| store.row(table, i)),
            ))
        }
        Node::IndexScan {
            table,
            column,
            low,
            high,
        } => {
            let table = *table;
            let index = store.index(table, *column);
            let from = index.partition_point(|e| e.0 < *low);
            let to = index.partition_point(|e| e.0 <= *high).max(from);
            Box::new(Rows(
                index[from..to].iter().map(move |e| store.row(table, e.1)),
            ))
        }
        Node::Filter(condition) => Box::new(Filtered {
            input: input(0),
            condition,
            schema: schema(plan, store),
        }),
        Node::Project(columns) => {
            let from = schema(&plan.inputs[0], store);
            let picks: Vec<usize> = columns.iter().map(|c| position(&from, *c)).collect();
            let mut source = input(0);
            Box::new(Rows(
                std::iter::from_fn(move || source.next())
                    .map(move |row| picks.iter().map(|&p| row[p]).collect()),
            ))
        }
        Node::NestedLoop(condition) => Box::new(NestedLoop {
            outer: input(0),
            unread: Some(input(1)),
            inner: Vec::new(),
            current: None,
            at: 0,
            condition: condition.as_ref(),
            schema: schema(plan, store),
        }),
        Node::HashJoin {
            left_key,
            right_key,
            rest,
        } => Box::new(HashJoin {
            probe: input(0),
            unread: Some(input(1)),
            table: HashMap::new(),
            pending: VecDeque::new(),
            keys: key_positions(plan, store, *left_key, *right_key),
            rest: rest.as_ref(),
            schema: schema(plan, store),
        }),
        Node::MergeJoin {
            left_key,
            right_key,
            rest,
        } => Box::new(MergeJoin {
            left: input(0),
            right: input(1),
            ahead: None,
            group: Vec::new(),
            group_key: None,
            current: None,
            at: 0,
            keys: key_positions(plan, store, *left_key, *right_key),
            rest: rest.as_ref(),
            schema: schema(plan, store),
        }),
        Node::Sort(column) => {
            let at = position(&schema(plan, store), *column);
            let mut source = Some(input(0));
            let mut sorted = Vec::new().into_iter();
            Box::new(Rows(std::iter::from_fn(move || {
                if let Some(input) = source.take() {
                    let mut rows = drain(input);
                    rows.sort_by_key(|row| row[at]);
                    sorted = rows.into_iter();
                }
                sorted.next()
            })))
        }
    }
}

/// Run a plan to the end.
pub fn run(plan: &Plan, store: &dyn Store) -> Vec<Row> {
    drain(open(plan, store))
}

// ---- costs, and the ways to read one table ----

pub const PAGE_ROWS: usize = 100;
pub const SEQ_PAGE: f64 = 1.0;
pub const RANDOM_PAGE: f64 = 4.0;
/// For every row a scan or a join produces.
pub const CPU_ROW: f64 = 0.01;
/// For every condition checked on a row, and every row a merge walks over.
pub const CPU_CHECK: f64 = 0.0025;
/// The pages read to find the first entry of an index.
pub const INDEX_DESCENT: f64 = 3.0;

fn pages(rows: usize) -> f64 {
    rows.div_ceil(PAGE_ROWS).max(1) as f64
}

/// Put a filter for `conditions` on top of `input`, unless there are none.
fn filtered(input: Plan, conditions: Vec<Cond>) -> Plan {
    if conditions.is_empty() {
        return input;
    }
    let cost = input.cost + input.rows * CPU_CHECK * conditions.len() as f64;
    Plan {
        rows: input.rows,
        cost,
        order: input.order,
        node: Node::Filter(combine(conditions)),
        inputs: vec![input],
    }
}

/// Every way to read the rows of `table` for which all the conditions hold.
pub fn leaf_plans(catalog: &Catalog, table: usize, conditions: &[Cond]) -> Vec<Plan> {
    let facts = &catalog.tables[table];
    let total = facts.rows as f64 * selectivity(&Cond::All(conditions.to_vec()), catalog);
    let finish = |mut plan: Plan| {
        plan.rows = total;
        plan
    };
    let mut scan = Plan::new(Node::Scan(table), Vec::new());
    scan.rows = facts.rows as f64;
    scan.cost = pages(facts.rows) * SEQ_PAGE + facts.rows as f64 * CPU_ROW;
    let mut plans = vec![finish(filtered(scan, conditions.to_vec()))];
    for &column in &facts.indexed {
        let key = Column { table, column };
        let (mut low, mut high, mut used) = (i64::MIN, i64::MAX, false);
        let mut rest = Vec::new();
        for condition in conditions {
            let range = match condition {
                Cond::Compare(c, cmp, value) if *c == key => range_of(*cmp, *value),
                _ => None,
            };
            match range {
                Some((from, to)) => {
                    (low, high, used) = (low.max(from), high.min(to), true);
                }
                None => rest.push(condition.clone()),
            }
        }
        if !used {
            continue;
        }
        let matched = facts.stats[column].between(low, high);
        let node = Node::IndexScan {
            table,
            column,
            low,
            high,
        };
        let mut index = Plan::new(node, Vec::new());
        index.rows = matched;
        index.cost =
            (INDEX_DESCENT + matched.min(pages(facts.rows))) * RANDOM_PAGE + matched * CPU_ROW;
        index.order = Some(key);
        plans.push(finish(filtered(index, rest)));
    }
    plans
}

// ---- the ways to join two plans ----

pub const HASH_BUILD: f64 = 0.02;
pub const HASH_PROBE: f64 = 0.01;
/// A sort of `n` rows costs `n * log2(n)` times this, with `n` counted as at least 2.
pub const SORT_STEP: f64 = 0.005;

/// The tables a plan reads, as bits.
pub fn plan_tables(plan: &Plan) -> u64 {
    match &plan.node {
        Node::Scan(table) | Node::IndexScan { table, .. } => 1 << table,
        _ => plan.inputs.iter().fold(0, |m, p| m | plan_tables(p)),
    }
}

/// `plan` itself if it is already in this order, else a sort on top of it.
fn sorted_by(plan: &Plan, key: Column) -> Plan {
    if plan.order == Some(key) {
        return plan.clone();
    }
    let n = plan.rows.max(2.0);
    Plan {
        node: Node::Sort(key),
        rows: plan.rows,
        cost: plan.cost + n * n.log2() * SORT_STEP,
        order: Some(key),
        inputs: vec![plan.clone()],
    }
}

/// The first condition that equates a column of the left plan with one of the right: its place in the list and
/// the two columns, left one first.
fn join_key(left: u64, right: u64, conditions: &[Cond]) -> Option<(usize, Column, Column)> {
    let in_set = |set: u64, c: &Column| set & (1 << c.table) != 0;
    conditions
        .iter()
        .enumerate()
        .find_map(|(i, condition)| match condition {
            Cond::Same(a, b) if in_set(left, a) && in_set(right, b) => Some((i, *a, *b)),
            Cond::Same(a, b) if in_set(left, b) && in_set(right, a) => Some((i, *b, *a)),
            _ => None,
        })
}

/// Every way to join `left` (outer) with `right` (inner), when `conditions` must hold between them and the
/// result has `rows` rows: a nested loop, then, if there is a key, a hash join and a merge join.
pub fn join_plans(left: &Plan, right: &Plan, conditions: &[Cond], rows: f64) -> Vec<Plan> {
    let all = (!conditions.is_empty()).then(|| combine(conditions.to_vec()));
    let mut plans = vec![Plan {
        node: Node::NestedLoop(all),
        rows,
        cost: left.cost + right.cost + left.rows * right.rows * CPU_CHECK + rows * CPU_ROW,
        order: left.order,
        inputs: vec![left.clone(), right.clone()],
    }];
    let key = join_key(plan_tables(left), plan_tables(right), conditions);
    let Some((used, left_key, right_key)) = key else {
        return plans;
    };
    let mut others = conditions.to_vec();
    others.remove(used);
    let extra = if others.is_empty() {
        0.0
    } else {
        rows * CPU_CHECK
    };
    let rest = (!others.is_empty()).then(|| combine(others));
    plans.push(Plan {
        node: Node::HashJoin {
            left_key,
            right_key,
            rest: rest.clone(),
        },
        rows,
        cost: left.cost
            + right.cost
            + right.rows * HASH_BUILD
            + left.rows * HASH_PROBE
            + rows * CPU_ROW
            + extra,
        order: None,
        inputs: vec![left.clone(), right.clone()],
    });
    let (a, b) = (sorted_by(left, left_key), sorted_by(right, right_key));
    plans.push(Plan {
        node: Node::MergeJoin {
            left_key,
            right_key,
            rest,
        },
        rows,
        cost: a.cost + b.cost + (a.rows + b.rows) * CPU_CHECK + rows * CPU_ROW + extra,
        order: Some(left_key),
        inputs: vec![a, b],
    });
    plans
}

/// The plans worth keeping for one set of tables: the cheapest for each output order.
#[derive(Debug, Clone, Default)]
pub struct PlanSet {
    plans: Vec<Plan>,
}

impl PlanSet {
    pub fn new() -> PlanSet {
        PlanSet::default()
    }

    pub fn insert(&mut self, plan: Plan) {
        match self.plans.iter_mut().find(|p| p.order == plan.order) {
            Some(kept) if kept.cost <= plan.cost => {}
            Some(kept) => *kept = plan,
            None => self.plans.push(plan),
        }
    }

    /// The kept plans, in no particular order.
    pub fn plans(&self) -> &[Plan] {
        &self.plans
    }

    pub fn cheapest(&self) -> Option<&Plan> {
        self.plans.iter().min_by(|a, b| a.cost.total_cmp(&b.cost))
    }
}
