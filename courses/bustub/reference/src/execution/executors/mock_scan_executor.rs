//! The built-in `__mock_*` tables. They have no storage: a function makes row number `i` on demand, so large or patterned data (a million rows,
//! NULLs in a pattern, a small graph) costs nothing to set up.

use super::abstract_executor::Executor;
use crate::catalog::column::Column;
use crate::catalog::schema::Schema;
use crate::common::exception::{Exception, ExceptionType, Result};
use crate::common::rid::Rid;
use crate::execution::plans::plan_node::{PlanKind, PlanRef};
use crate::storage::table::tuple::Tuple;
use crate::types::type_id::TypeId;
use crate::types::value::Value;

pub const MOCK_TABLE_LIST: &[&str] = &[
    "__mock_table_1",
    "__mock_table_2",
    "__mock_table_3",
    "__mock_table_4",
    "__mock_table_tas_2022",
    "__mock_table_tas_2023",
    "__mock_table_tas_2023_fall",
    "__mock_table_tas_2024",
    "__mock_table_tas_2024_fall",
    "__mock_table_tas_2025_spring",
    "__mock_table_tas_2025_fall",
    "__mock_agg_input_small",
    "__mock_agg_input_big",
    "__mock_external_merge_sort_input",
    "__mock_table_schedule_2022",
    "__mock_table_schedule",
    "__mock_table_123",
    "__mock_graph",
    "__mock_t1",
    "__mock_t4_1m",
    "__mock_t5_1m",
    "__mock_t6_1m",
    "__mock_t7",
    "__mock_t8",
    "__mock_t9",
    "__mock_t10",
    "__mock_t11",
];

const TA_LIST_2022: &[&str] =
    &["amstqq", "durovo", "joyceliaoo", "karthik-ramanathan-3006", "kush789", "lmwnshn", "mkpjnx", "skyzh", "thepinetree", "timlee0119", "yliang412"];
const TA_LIST_2023: &[&str] = &["abigalekim", "arvinwu168", "christopherlim98", "David-Lyons", "fanyuex2", "Mayank-Baranwal", "skyzh", "yarkhinephyo", "yliang412"];
const TA_LIST_2023_FALL: &[&str] =
    &["skyzh", "yliang412", "fernandolis10", "wiam8", "anurag-23", "Mayank-Baranwal", "abigalekim", "ChaosZhai", "aoleizhou", "averyqi115", "kswim8"];
const TA_LIST_2024: &[&str] = &["AlSchlo", "walkingcabbages", "averyqi115", "lanlou1554", "sweetsuro", "ChaosZhai", "SDTheSlayer", "xx01cyx", "yliang412", "thelongmarch-azx"];
const TA_LIST_2024_FALL: &[&str] = &["17zhangw", "connortsui20", "J-HowHuang", "lanlou1554", "prashanthduvvada", "unw9527", "xx01cyx", "yashkothari42"];
const TA_LIST_2025_SPRING: &[&str] = &["AlSchlo", "carpecodeum", "ChrisLaspias", "hyoungjook", "joesunil123", "mrwhitezz", "rmboyce", "yliang412"];
const TA_LIST_2025_FALL: &[&str] = &["17zhangw", "quantumish", "songwdfu", "notSaranshMalik", "shinyumh", "s-wangru", "rayhhome", "MrWhitezz"];
const TA_OH_2022: &[&str] = &["Tuesday", "Wednesday", "Monday", "Wednesday", "Thursday", "Friday", "Wednesday", "Randomly", "Tuesday", "Monday", "Tuesday"];
const TA_OH_2023: &[&str] = &["Friday", "Thursday", "Tuesday", "Monday", "Tuesday", "Tuesday", "Randomly", "Wednesday", "Thursday"];
const TA_OH_2023_FALL: &[&str] = &["Randomly", "Tuesday", "Wednesday", "Tuesday", "Thursday", "Tuesday", "Friday", "Yesterday", "Friday", "Friday", "Never"];
const TA_OH_2024: &[&str] = &["Friday", "Thursday", "Friday", "Wednesday", "Thursday", "Yesterday", "Monday", "Tuesday", "Tuesday", "Monday"];
const TA_OH_2024_FALL: &[&str] = &["Wednesday", "Thursday", "Tuesday", "Monday", "Friday", "Thursday", "Tuesday", "Friday"];
const TA_OH_2025_SPRING: &[&str] = &["Friday", "Monday", "Wednesday", "Tuesday", "Friday", "Thursday", "Monday", "Tuesday"];
const TA_OH_2025_FALL: &[&str] = &["Tuesday", "Monday", "Thursday", "Friday", "Tuesday", "Tuesday", "Friday", "Wednesday"];
const COURSE_ON_DATE: &[&str] = &["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const GRAPH_NODE_CNT: usize = 10;

fn ints(names: &[&str]) -> Schema {
    Schema::new(names.iter().map(|n| Column::new(n, TypeId::Integer)).collect())
}

/// The schema of a mock table.
pub fn get_mock_table_schema_of(table: &str) -> Result<Schema> {
    let vc = |n: &str| Column::new_varchar(n, 128);
    let int = |n: &str| Column::new(n, TypeId::Integer);
    Ok(match table {
        "__mock_table_1" => ints(&["colA", "colB"]),
        "__mock_table_2" => Schema::new(vec![vc("colC"), vc("colD")]),
        "__mock_table_3" => Schema::new(vec![int("colE"), vc("colF")]),
        "__mock_table_4" => Schema::new(vec![int("colG"), vc("colH")]),
        "__mock_table_tas_2022" | "__mock_table_tas_2023" | "__mock_table_tas_2023_fall" | "__mock_table_tas_2024" | "__mock_table_tas_2024_fall"
        | "__mock_table_tas_2025_spring" | "__mock_table_tas_2025_fall" => Schema::new(vec![vc("github_id"), vc("office_hour")]),
        "__mock_table_schedule_2022" | "__mock_table_schedule" => Schema::new(vec![vc("day_of_week"), int("has_lecture")]),
        "__mock_agg_input_small" | "__mock_agg_input_big" => Schema::new(vec![int("v1"), int("v2"), int("v3"), int("v4"), int("v5"), vc("v6")]),
        "__mock_external_merge_sort_input" => ints(&["v1", "v2", "v3"]),
        "__mock_graph" => ints(&["src", "dst", "src_label", "dst_label", "distance"]),
        "__mock_table_123" => ints(&["number"]),
        "__mock_t4_1m" | "__mock_t5_1m" | "__mock_t6_1m" | "__mock_t9" | "__mock_t10" | "__mock_t11" => ints(&["x", "y"]),
        "__mock_t1" => ints(&["x", "y", "z"]),
        "__mock_t7" => ints(&["v", "v1", "v2"]),
        "__mock_t8" => ints(&["v4"]),
        other => return Err(Exception::new(ExceptionType::Invalid, format!("mock table {other} not found"))),
    })
}

fn get_size_of(table: &str) -> usize {
    match table {
        "__mock_table_1" | "__mock_table_2" | "__mock_table_3" | "__mock_table_4" => 100,
        "__mock_table_tas_2022" => TA_LIST_2022.len(),
        "__mock_table_tas_2023" => TA_LIST_2023.len(),
        "__mock_table_tas_2023_fall" => TA_LIST_2023_FALL.len(),
        "__mock_table_tas_2024" => TA_LIST_2024.len(),
        "__mock_table_tas_2024_fall" => TA_LIST_2024_FALL.len(),
        "__mock_table_tas_2025_spring" => TA_LIST_2025_SPRING.len(),
        "__mock_table_tas_2025_fall" => TA_LIST_2025_FALL.len(),
        "__mock_table_schedule_2022" | "__mock_table_schedule" => COURSE_ON_DATE.len(),
        "__mock_agg_input_small" => 1000,
        "__mock_agg_input_big" => 10000,
        "__mock_external_merge_sort_input" => 100000,
        "__mock_graph" => GRAPH_NODE_CNT * GRAPH_NODE_CNT,
        "__mock_table_123" => 3,
        "__mock_t1" | "__mock_t4_1m" | "__mock_t5_1m" | "__mock_t6_1m" | "__mock_t7" | "__mock_t11" => 1_000_000,
        "__mock_t8" => 10,
        "__mock_t9" => 10_000_000,
        "__mock_t10" => 10_000,
        _ => 0,
    }
}

/// Row `cursor` of a mock table, as values.
fn mock_row(table: &str, cursor: usize, schema: &Schema) -> Vec<Value> {
    let int = |v: i64| Value::integer(v as i32);
    let s = Value::varchar;
    match table {
        "__mock_table_tas_2022" => vec![s(TA_LIST_2022[cursor]), s(TA_OH_2022[cursor])],
        "__mock_table_tas_2023" => vec![s(TA_LIST_2023[cursor]), s(TA_OH_2023[cursor])],
        "__mock_table_tas_2023_fall" => vec![s(TA_LIST_2023_FALL[cursor]), s(TA_OH_2023_FALL[cursor])],
        "__mock_table_tas_2024" => vec![s(TA_LIST_2024[cursor]), s(TA_OH_2024[cursor])],
        "__mock_table_tas_2024_fall" => vec![s(TA_LIST_2024_FALL[cursor]), s(TA_OH_2024_FALL[cursor])],
        "__mock_table_tas_2025_spring" => vec![s(TA_LIST_2025_SPRING[cursor]), s(TA_OH_2025_SPRING[cursor])],
        "__mock_table_tas_2025_fall" => vec![s(TA_LIST_2025_FALL[cursor]), s(TA_OH_2025_FALL[cursor])],
        "__mock_table_schedule_2022" => vec![s(COURSE_ON_DATE[cursor]), int(if cursor == 1 || cursor == 3 { 1 } else { 0 })],
        "__mock_table_schedule" => vec![s(COURSE_ON_DATE[cursor]), int(if cursor == 0 || cursor == 2 { 1 } else { 0 })],
        "__mock_table_1" => vec![int(cursor as i64), int(cursor as i64 * 100)],
        "__mock_table_2" => vec![s(&format!("{cursor}-\u{1F4A9}")), s(&"\u{1F607}".repeat(cursor % 8))],
        "__mock_table_3" => vec![if cursor % 2 == 0 { int(cursor as i64) } else { Value::null(TypeId::Integer) }, s(&format!("{cursor}-\u{1F4A9}"))],
        "__mock_table_4" => {
            let mut text = "\u{1F4A9}".to_string();
            for _ in 0..(cursor % 3) {
                text.push('\u{1F4A9}');
            }
            vec![
                if cursor % 5 != 0 { int((cursor % 5) as i64) } else { Value::null(TypeId::Integer) },
                if cursor % 10 != 0 { s(&text) } else { Value::null(TypeId::Varchar) },
            ]
        }
        "__mock_agg_input_small" => vec![
            int(((cursor + 2) % 10) as i64),
            int(cursor as i64),
            int(((cursor + 50) % 100) as i64),
            int((cursor / 100) as i64),
            int(233),
            s(&"\u{1F4A9}".repeat(cursor % 8 + 1)),
        ],
        "__mock_agg_input_big" => vec![
            int(((cursor + 2) % 10) as i64),
            int(cursor as i64),
            int(((cursor + 50) % 100) as i64),
            int((cursor / 1000) as i64),
            int(233),
            s(&"\u{1F4A9}".repeat(cursor % 16 + 1)),
        ],
        "__mock_external_merge_sort_input" => vec![int(cursor as i64), int(((cursor + 1777) % 15000) as i64), int(((cursor + 3) % 111) as i64)],
        "__mock_table_123" => vec![int(cursor as i64 + 1)],
        "__mock_graph" => {
            let (src, dst) = (cursor % GRAPH_NODE_CNT, cursor / GRAPH_NODE_CNT);
            vec![
                int(src as i64),
                int(dst as i64),
                int(src as i64 * 100),
                int(dst as i64 * 100),
                if src == dst { Value::null(TypeId::Integer) } else { int(1) },
            ]
        }
        "__mock_t1" => vec![int((cursor / 10000) as i64), int((cursor % 10000) as i64), int(cursor as i64)],
        "__mock_t8" => vec![int(cursor as i64)],
        "__mock_t4_1m" => {
            let c = (cursor % 500000) as i64;
            vec![int(c), int(c * 10)]
        }
        "__mock_t5_1m" => {
            let c = ((cursor + 30000) % 500000) as i64;
            vec![int(c), int(c * 10)]
        }
        "__mock_t6_1m" => {
            let c = ((cursor + 60000) % 500000) as i64;
            vec![int(c), int(c * 10)]
        }
        "__mock_t7" => vec![int((cursor % 20) as i64), int(cursor as i64), int(cursor as i64)],
        "__mock_t9" => {
            let c = cursor as i64;
            vec![int(c / 10000), int(10_000_000 - (c / 2 + ((c / 10000) % 2) * ((c / 2) % 2)))]
        }
        "__mock_t10" => vec![int(cursor as i64), int(cursor as i64 * 10)],
        "__mock_t11" => vec![int(-1 * (cursor as i64 % 1000) - 1), int(cursor as i64 * 10)],
        _ => schema
            .columns()
            .iter()
            .map(|c| match c.type_id() {
                TypeId::Integer => Value::integer(0),
                TypeId::Varchar => Value::varchar(""),
                t => Value::null(t),
            })
            .collect(),
    }
}

pub struct MockScanExecutor {
    plan: PlanRef,
    table: String,
    size: usize,
    cursor: usize,
    /// For the tables whose rows come in random order.
    shuffled_idx: Vec<usize>,
}

impl MockScanExecutor {
    pub fn new(plan: PlanRef) -> MockScanExecutor {
        let PlanKind::MockScan { table } = &plan.kind else { unreachable!("a MockScanExecutor needs a MockScan plan") };
        let table = table.clone();
        let size = get_size_of(&table);
        let mut shuffled_idx = vec![];
        if table == "__mock_t1" {
            shuffled_idx = (0..size).collect();
            // a small xorshift generator; the order only has to be "not sorted"
            let mut state = 0x9E37_79B9_7F4A_7C15u64;
            for i in (1..shuffled_idx.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                shuffled_idx.swap(i, (state % (i as u64 + 1)) as usize);
            }
        }
        MockScanExecutor { plan, table, size, cursor: 0, shuffled_idx }
    }
}

impl Executor for MockScanExecutor {
    fn init(&mut self) -> Result<()> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self, tuple_batch: &mut Vec<Tuple>, rid_batch: &mut Vec<Rid>, batch_size: usize) -> Result<bool> {
        tuple_batch.clear();
        rid_batch.clear();
        while tuple_batch.len() < batch_size && self.cursor < self.size {
            let row = if self.shuffled_idx.is_empty() { self.cursor } else { self.shuffled_idx[self.cursor] };
            self.cursor += 1;
            tuple_batch.push(Tuple::new(&mock_row(&self.table, row, &self.plan.output_schema), &self.plan.output_schema));
            rid_batch.push(Rid::default());
        }
        Ok(!tuple_batch.is_empty())
    }

    fn output_schema(&self) -> &Schema {
        &self.plan.output_schema
    }
}
