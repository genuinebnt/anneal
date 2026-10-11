//! The `test_*` tables a fresh instance starts with (`test_simple_seq_1` holds the numbers 0 to 9).
//!
//! Unlike the `__mock_*` tables they are real tables in the buffer pool.

use super::catalog::Catalog;
use super::column::Column;
use super::schema::Schema;
use crate::common::exception::Result;
use crate::storage::table::tuple::{Tuple, TupleMeta};
use crate::types::type_id::TypeId;
use crate::types::value::Value;

pub const TEST1_SIZE: u32 = 1000;
pub const TEST2_SIZE: u32 = 100;
pub const TEST3_SIZE: u32 = 100;
pub const TEST4_SIZE: u32 = 100;
pub const TEST6_SIZE: u32 = 100;
pub const TEST7_SIZE: u32 = 100;
pub const TEST8_SIZE: u32 = 10;
pub const TEST9_SIZE: u32 = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dist {
    Uniform,
    Serial,
    Cyclic,
}

struct ColumnInsertMeta {
    name: &'static str,
    dist: Dist,
    min: i32,
    max: i32,
    serial_counter: i32,
}

struct TableInsertMeta {
    name: &'static str,
    num_rows: u32,
    cols: Vec<ColumnInsertMeta>,
}

fn col(name: &'static str, dist: Dist, min: i32, max: i32) -> ColumnInsertMeta {
    ColumnInsertMeta { name, dist, min, max, serial_counter: 0 }
}

fn make_values(col: &mut ColumnInsertMeta, count: u32, rng: &mut u64) -> Vec<Value> {
    (0..count)
        .map(|_| match col.dist {
            Dist::Serial => {
                let v = col.serial_counter + col.min;
                col.serial_counter += 1;
                Value::integer(v)
            }
            Dist::Cyclic => {
                let v = col.serial_counter;
                col.serial_counter += 1;
                if col.serial_counter > col.max {
                    col.serial_counter = 0;
                }
                Value::integer(v)
            }
            Dist::Uniform => {
                *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let span = (col.max as i64 - col.min as i64 + 1).max(1) as u64;
                Value::integer(col.min + ((*rng >> 33) % span) as i32)
            }
        })
        .collect()
}

pub fn generate_test_tables(catalog: &mut Catalog<'_>) -> Result<()> {
    let metas = vec![
        TableInsertMeta { name: "empty_table", num_rows: 0, cols: vec![col("colA", Dist::Serial, 0, 0)] },
        TableInsertMeta { name: "test_simple_seq_1", num_rows: 10, cols: vec![col("col1", Dist::Serial, 0, 10)] },
        TableInsertMeta { name: "test_simple_seq_2", num_rows: 10, cols: vec![col("col1", Dist::Serial, 0, 10), col("col2", Dist::Serial, 10, 20)] },
        TableInsertMeta {
            name: "test_1",
            num_rows: TEST1_SIZE,
            cols: vec![col("colA", Dist::Serial, 0, 0), col("colB", Dist::Uniform, 0, 9), col("colC", Dist::Uniform, 0, 9999), col("colD", Dist::Uniform, 0, 99999)],
        },
        TableInsertMeta {
            name: "test_2",
            num_rows: TEST7_SIZE,
            cols: vec![col("colA", Dist::Serial, 0, 99), col("colB", Dist::Uniform, 0, 999), col("colC", Dist::Cyclic, 0, 9)],
        },
    ];
    for mut meta in metas {
        let schema = Schema::new(meta.cols.iter().map(|c| Column::new(c.name, TypeId::Integer)).collect());
        let info = catalog.create_table(meta.name, &schema).expect("the test tables have distinct names");
        let (mut inserted, mut rng) = (0u32, 42u64);
        while inserted < meta.num_rows {
            let n = 128.min(meta.num_rows - inserted);
            let columns: Vec<Vec<Value>> = meta.cols.iter_mut().map(|c| make_values(c, n, &mut rng)).collect();
            for i in 0..n as usize {
                let row: Vec<Value> = columns.iter().map(|c| c[i].clone()).collect();
                info.table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &Tuple::new(&row, &info.schema))?;
                inserted += 1;
            }
        }
    }
    Ok(())
}
