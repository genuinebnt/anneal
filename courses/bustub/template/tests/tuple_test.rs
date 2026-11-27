//! Port of `test/table/tuple_test.cpp` (BusTub, MIT, Copyright (c) 2015-2025 Carnegie Mellon University Database Group): `TableHeapTest`
//! inserts the same tuple 5,000 times into a table in a 50-frame buffer pool and walks the table with an iterator. This port also checks
//! what the C++ test only prints: every inserted rid is found again and the iterator yields exactly the inserted tuples. The disk is
//! in memory (BusTub's test uses a file that it removes afterwards), plus a catalog-and-index round trip that module 3c adds.

use std::sync::Arc;

use bustub::buffer::buffer_pool_manager::BufferPoolManager;
use bustub::catalog::catalog::Catalog;
use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::rid::Rid;
use bustub::storage::disk::disk_manager_memory::DiskManagerUnlimitedMemory;
use bustub::storage::table::table_heap::TableHeap;
use bustub::storage::table::tuple::{Tuple, TupleMeta};
use bustub::types::type_id::TypeId::*;
use bustub::types::value::Value;

/// BusTub's `ConstructTuple` (from `test/include/...`): a value for each column of the schema, derived from the column's type.
fn construct_tuple(schema: &Schema) -> Tuple {
    let mut values = vec![];
    let mut seed = 0;
    for column in schema.columns() {
        seed += 1;
        values.push(match column.type_id() {
            Boolean => Value::boolean(seed % 2 == 0),
            TinyInt => Value::tinyint(seed as i8),
            SmallInt => Value::smallint(seed as i16),
            Integer => Value::integer(seed),
            BigInt => Value::bigint(seed as i64),
            Decimal => Value::decimal(seed as f64),
            Timestamp => Value::timestamp(seed as u64),
            Varchar => Value::varchar(&"abcdefghijklmnopqrstuvwxyz"[..(seed as usize % 10) + 1]),
            Invalid => unreachable!(),
        });
    }
    Tuple::new(&values, schema)
}

#[test]
fn table_heap_test() {
    // test1: parse create sql statement
    let schema = Schema::new(vec![
        Column::new_varchar("a", 20),
        Column::new("b", SmallInt),
        Column::new("c", BigInt),
        Column::new("d", Boolean),
        Column::new_varchar("e", 16),
    ]);
    let tuple = construct_tuple(&schema);

    let buffer_pool_manager = BufferPoolManager::new(50, Arc::new(DiskManagerUnlimitedMemory::new()));
    let table = TableHeap::new(&buffer_pool_manager);

    let mut rid_v: Vec<Rid> = Vec::new();
    for _ in 0..5000 {
        let rid = table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &tuple);
        if let Ok(rid) = rid {
            rid_v.push(rid);
        }
    }
    assert_eq!(rid_v.len(), 5000);

    let mut itr = table.make_iterator();
    let mut n = 0;
    while !itr.is_end() {
        let (meta, t) = itr.get_tuple().unwrap();
        assert!(!meta.is_deleted);
        assert_eq!(t.data(), tuple.data());
        assert_eq!(t.to_string(&schema), tuple.to_string(&schema)); // BusTub's commented-out `itr->ToString(schema)`
        assert_eq!(itr.get_rid(), rid_v[n]);
        itr.advance();
        n += 1;
    }
    assert_eq!(n, 5000);
}

#[test]
fn a_table_and_an_index_through_the_catalog() {
    let bpm = BufferPoolManager::new(50, Arc::new(DiskManagerUnlimitedMemory::new()));
    let mut catalog = Catalog::new(&bpm);
    let schema = Schema::new(vec![Column::new("id", Integer), Column::new_varchar("name", 16), Column::new("score", Integer)]);
    let info = catalog.create_table("people", &schema).unwrap();
    for i in 0..2000 {
        let row = Tuple::new(&[Value::integer(i), Value::varchar(&format!("person-{i}")), Value::integer(i % 50)], &schema);
        info.table.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &row).unwrap();
    }
    let by_id = catalog.create_index("people_id", "people", vec![0], true).unwrap().unwrap();
    let by_score = catalog.create_index("people_score", "people", vec![2], false).unwrap().unwrap();
    // point lookups go index -> rid -> tuple
    for id in [0, 1, 999, 1999] {
        let rids = by_id.index.scan_key(&Tuple::new(&[Value::integer(id)], &by_id.key_schema));
        assert_eq!(rids.len(), 1);
        let (_, row) = info.table.get_tuple(rids[0]).unwrap();
        assert_eq!(row.get_value(&schema, 1), Value::varchar(&format!("person-{id}")));
    }
    // an index that is not a primary key takes repeated keys: each score has 40 rows, found in rid order
    let rids = by_score.index.scan_key(&Tuple::new(&[Value::integer(7)], &by_score.key_schema));
    assert_eq!(rids.len(), 40, "the score 7 finds its 40 rows");
    assert!(rids.windows(2).all(|w| w[0] < w[1]), "the rows of one key come in rid order");
    for rid in &rids {
        assert_eq!(info.table.get_tuple(*rid).unwrap().1.get_value(&schema, 2), Value::integer(7), "each leads to a row with the score 7");
    }
    // the index scan is ordered by key: ids come back 0, 1, 2, ...
    let ids: Vec<i64> =
        by_id.index.scan_all().iter().map(|(_, r)| info.table.get_tuple(*r).unwrap().1.get_value(&schema, 0).as_i64().unwrap()).collect();
    assert_eq!(ids, (0..2000).collect::<Vec<i64>>());
}
