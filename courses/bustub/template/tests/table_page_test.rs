//! The boss of module 3b. BusTub has no unit test for `TablePage` on its own (its `tmp_tuple_page_test` is `DISABLED_` and for an obsolete
//! page type; it is exercised through `TableHeapTest`, module 3c's boss), so this file is the course's: schemas, tuples and a table page used
//! together, the way the table heap will use them.

use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::config::{PageId, BUSTUB_PAGE_SIZE};
use bustub::common::rid::Rid;
use bustub::storage::page::table_page::TablePage;
use bustub::storage::table::tuple::{Tuple, TupleMeta};
use bustub::types::type_id::TypeId::*;
use bustub::types::value::Value;

/// BusTub's `TableHeapTest` schema: `a varchar(20), b smallint, c bigint, d bool, e varchar(16)`.
fn heap_test_schema() -> Schema {
    Schema::new(vec![
        Column::new_varchar("a", 20),
        Column::new("b", SmallInt),
        Column::new("c", BigInt),
        Column::new("d", Boolean),
        Column::new_varchar("e", 16),
    ])
}

fn row(i: usize) -> Vec<Value> {
    vec![
        Value::varchar(&format!("a{i}")),
        Value::smallint((i % 1000) as i16),
        Value::bigint(i as i64 * 1_000_003),
        Value::boolean(i % 2 == 0),
        if i % 7 == 0 { Value::null(Varchar) } else { Value::varchar(&"e".repeat(i % 16)) },
    ]
}

#[test]
fn a_page_of_tuples_comes_back_value_by_value() {
    let schema = heap_test_schema();
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    let mut stored = 0;
    while let Some(slot) = page.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &Tuple::new(&row(stored), &schema)) {
        assert_eq!(slot as usize, stored);
        stored += 1;
    }
    assert!(stored > 100, "a page holds many small tuples (stored {stored})");
    for i in 0..stored {
        let (meta, t) = page.get_tuple(Rid::new(PageId(1), i as u32)).unwrap();
        assert_eq!(meta, TupleMeta { ts: 0, is_deleted: false });
        let values: Vec<Value> = (0..5).map(|c| t.get_value(&schema, c)).collect();
        assert_eq!(values, row(i), "tuple {i}");
    }
}

#[test]
fn deleting_marks_but_does_not_move_anything() {
    let schema = heap_test_schema();
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    for i in 0..20 {
        page.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &Tuple::new(&row(i), &schema));
    }
    for i in (0..20).step_by(3) {
        page.update_tuple_meta(&TupleMeta { ts: 0, is_deleted: true }, Rid::new(PageId(1), i)).unwrap();
    }
    assert_eq!(page.get_num_deleted_tuples(), 7);
    for i in 0..20 {
        let (meta, t) = page.get_tuple(Rid::new(PageId(1), i as u32)).unwrap();
        assert_eq!(meta.is_deleted, i % 3 == 0);
        assert_eq!(t.get_value(&schema, 0), row(i)[0], "every tuple is where it was");
    }
}

#[test]
fn updating_in_place_keeps_the_length_and_changes_the_value() {
    let schema = Schema::new(vec![Column::new("id", Integer), Column::new("score", BigInt)]);
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    let rid = Rid::new(
        PageId(1),
        page.insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &Tuple::new(&[Value::integer(1), Value::bigint(10)], &schema)).unwrap()
            as u32,
    );
    let new = Tuple::new(&[Value::integer(1), Value::bigint(99)], &schema);
    page.update_tuple_in_place_unsafe(&TupleMeta { ts: 1, is_deleted: false }, &new, rid).unwrap();
    let (meta, t) = page.get_tuple(rid).unwrap();
    assert_eq!((meta.ts, t.get_value(&schema, 1)), (1, Value::bigint(99)));
}

#[test]
fn a_table_is_a_chain_of_pages() {
    // what the table heap does: when a page is full, start another and link it
    let schema = heap_test_schema();
    let mut pages: Vec<[u8; BUSTUB_PAGE_SIZE]> = vec![[0; BUSTUB_PAGE_SIZE]];
    TablePage::new(&mut pages[0][..]).init();
    let mut rids = vec![];
    for i in 0..1000 {
        let tuple = Tuple::new(&row(i), &schema);
        let last = pages.len() - 1;
        let slot = match TablePage::new(&mut pages[last][..]).insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &tuple) {
            Some(slot) => slot,
            None => {
                pages.push([0; BUSTUB_PAGE_SIZE]);
                let next = pages.len() - 1;
                TablePage::new(&mut pages[next][..]).init();
                TablePage::new(&mut pages[last][..]).set_next_page_id(Some(PageId(next as i32)));
                TablePage::new(&mut pages[next][..]).insert_tuple(&TupleMeta { ts: 0, is_deleted: false }, &tuple).unwrap()
            }
        };
        rids.push(Rid::new(PageId(pages.len() as i32 - 1), slot as u32));
    }
    assert!(pages.len() > 2);
    // follow the chain from page 0 and count the tuples
    let (mut at, mut total) = (Some(PageId(0)), 0);
    while let Some(id) = at {
        let page = TablePage::new(&pages[id.0 as usize][..]);
        total += page.get_num_tuples();
        at = page.get_next_page_id();
    }
    assert_eq!(total, 1000);
    for (i, rid) in rids.iter().enumerate() {
        let page = TablePage::new(&pages[rid.page_id().0 as usize][..]);
        let (_, t) = page.get_tuple(*rid).unwrap();
        assert_eq!(t.get_value(&schema, 2), Value::bigint(i as i64 * 1_000_003));
    }
}
