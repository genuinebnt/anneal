//! Tests for module 3b, schemas, tuples and table pages. A test name starts with its stage: `s3b_02_…` belongs to stage 3b-02, and
//! `anneal course test` runs just those.
//!
//! Random schemas and rows are built from a strategy: columns of every type (and variable-length text), values that match (NULLs
//! included). Tuples are checked by round trip: whatever row went in comes out of `get_value` column by column. A table page is
//! checked against a plain `Vec` of what it should hold; how many tuples fit is checked against generous bounds, not a layout (the page's
//! layout is yours). Page tests copy the page's bytes to a fresh buffer to prove the page is self-describing: all its state is in its bytes.

use bustub::catalog::column::Column;
use bustub::catalog::schema::Schema;
use bustub::common::config::{PageId, BUSTUB_PAGE_SIZE};
use bustub::common::exception::ExceptionType;
use bustub::common::rid::Rid;
use bustub::storage::page::table_page::TablePage;
use bustub::storage::table::tuple::{Tuple, TupleMeta};
use bustub::types::type_id::TypeId::{self, *};
use bustub::types::value::{CmpBool, Value};
use proptest::prelude::*;

fn config() -> ProptestConfig {
    ProptestConfig { cases: 128, max_shrink_iters: 2000, failure_persistence: None, ..ProptestConfig::default() }
}

fn meta(is_deleted: bool) -> TupleMeta {
    TupleMeta { ts: 0, is_deleted }
}

fn same(a: &Value, b: &Value) -> bool {
    a.type_id() == b.type_id() && a.compare_exactly_equals(b)
}

// ---- Random schemas and rows ----------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Col {
    Fixed(TypeId),
    Text(u32),
}

fn col() -> impl Strategy<Value = Col> {
    prop_oneof![
        3 => prop::sample::select(vec![Boolean, TinyInt, SmallInt, Integer, BigInt, Decimal, Timestamp]).prop_map(Col::Fixed),
        2 => (1u32..40).prop_map(Col::Text),
    ]
}

fn schema_of(cols: &[Col]) -> Schema {
    Schema::new(cols.iter().enumerate().map(|(i, c)| match c {
        Col::Fixed(t) => Column::new(&format!("c{i}"), *t),
        Col::Text(n) => Column::new_varchar(&format!("c{i}"), *n),
    }).collect())
}

/// A value for a column from some random bits; about one in six is a NULL.
fn value_for(c: &Col, bits: u64) -> Value {
    if bits % 6 == 0 {
        return match c { Col::Fixed(t) => Value::null(*t), Col::Text(_) => Value::null(Varchar) };
    }
    match c {
        Col::Fixed(Boolean) => Value::boolean(bits >> 8 & 1 == 1),
        Col::Fixed(TinyInt) => Value::tinyint((bits >> 8) as i8),
        Col::Fixed(SmallInt) => Value::smallint((bits >> 8) as i16),
        Col::Fixed(Integer) => Value::integer((bits >> 8) as i32),
        Col::Fixed(BigInt) => Value::bigint((bits >> 8) as i64),
        Col::Fixed(Decimal) => Value::decimal(((bits >> 8) % 1_000_000) as f64 / 8.0 - 60_000.0),
        Col::Fixed(Timestamp) => Value::timestamp((bits >> 8) % 1_000_000_000),
        Col::Text(n) => Value::varchar(&"é𝄞x".chars().cycle().take(((bits >> 8) % (*n as u64 + 1)) as usize % 12).collect::<String>()),
        Col::Fixed(_) => unreachable!(),
    }
}

fn schema_and_rows() -> impl Strategy<Value = (Vec<Col>, Vec<Vec<u64>>)> {
    prop::collection::vec(col(), 1..8).prop_flat_map(|cols| {
        let n = cols.len();
        (Just(cols), prop::collection::vec(prop::collection::vec(any::<u64>(), n), 1..12))
    })
}

fn row(cols: &[Col], bits: &[u64]) -> Vec<Value> {
    cols.iter().zip(bits).map(|(c, b)| value_for(c, *b)).collect()
}

// ---- 3b-01 · Columns and schemas ------------------------------------------------------------------------------------------

#[test]
fn s3b_01_a_column_has_its_types_size_or_its_declared_length() {
    for (t, size) in [(Boolean, 1), (TinyInt, 1), (SmallInt, 2), (Integer, 4), (BigInt, 8), (Decimal, 8), (Timestamp, 8)] {
        let c = Column::new("x", t);
        assert_eq!((c.name(), c.type_id(), c.storage_size(), c.offset(), c.is_inlined()), ("x", t, size, 0, true), "{t:?}");
    }
    let v = Column::new_varchar("name", 32);
    assert_eq!((v.type_id(), v.storage_size(), v.is_inlined()), (Varchar, 32, false));
    let r = v.with_column_name("renamed");
    assert_eq!((r.name(), r.type_id(), r.storage_size()), ("renamed", Varchar, 32));
    assert_eq!(v.name(), "name");
}

#[test]
fn s3b_01_wrong_constructors_are_bugs_and_panic() {
    assert!(std::panic::catch_unwind(|| Column::new("name", Varchar)).is_err(), "a varchar needs a length");
    assert!(std::panic::catch_unwind(|| Column::new("x", Invalid)).is_err(), "the invalid type is not a column type");
    assert!(std::panic::catch_unwind(|| Schema::new(vec![Column::new("a", Integer)]).col_idx("nope")).is_err(), "a missing column by name is a bug");
}

#[test]
fn s3b_01_columns_and_schemas_print_like_bustub() {
    assert_eq!(Column::new("a", Integer).to_string(true), "a:INTEGER");
    assert_eq!(Column::new_varchar("b", 20).to_string(true), "b:VARCHAR(20)");
    assert_eq!(Column::new("c", BigInt).to_string(false), "Column[c, BIGINT, Offset:0, Length:8]");
    let s = Schema::new(vec![Column::new("a", Integer), Column::new_varchar("b", 20)]);
    assert_eq!(s.to_string(true), "(a:INTEGER, b:VARCHAR(20))");
    assert_eq!(s.to_string(false), "Schema[NumColumns:2, IsInlined:0, Length:8] :: (Column[a, INTEGER, Offset:0, Length:4], Column[b, VARCHAR, Offset:4, Length:20])");
    let e = Schema::new(vec![]);
    assert_eq!((e.column_count(), e.inlined_storage_size(), e.is_inlined(), e.to_string(true).as_str()), (0, 0, true, "()"));
}

proptest! {
    #![proptest_config(config())]

    /// Each column starts where the previous one ended: a fixed column takes its type's size, a text column a 4-byte slot (where its bytes
    /// are found); the schema's length is the total; it knows which columns are not inlined.
    #[test]
    fn s3b_01_columns_get_consecutive_offsets(cols in prop::collection::vec(col(), 0..10)) {
        let s = schema_of(&cols);
        let mut at = 0;
        let mut text = vec![];
        for (i, c) in cols.iter().enumerate() {
            prop_assert_eq!(s.column(i as u32).offset(), at, "column {}", i);
            at += match c { Col::Fixed(t) => t.type_size().unwrap() as u32, Col::Text(_) => { text.push(i as u32); 4 } };
        }
        prop_assert_eq!(s.inlined_storage_size(), at);
        prop_assert_eq!(s.uninlined_columns(), &text[..]);
        prop_assert_eq!((s.column_count() as usize, s.uninlined_column_count() as usize, s.is_inlined()), (cols.len(), text.len(), text.is_empty()));
    }

    /// Columns are found by name (the first one with that name); `copy_schema` picks columns in the order given and recomputes the offsets.
    #[test]
    fn s3b_01_copy_schema_picks_columns_and_lays_them_out_again(cols in prop::collection::vec(col(), 1..9), picks in prop::collection::vec(0usize..9, 0..8)) {
        let s = schema_of(&cols);
        let attrs: Vec<u32> = picks.iter().map(|p| (p % cols.len()) as u32).collect();
        let k = Schema::copy_schema(&s, &attrs);
        prop_assert_eq!(k.column_count() as usize, attrs.len());
        let direct = Schema::new(attrs.iter().map(|&i| s.column(i).clone()).collect());
        prop_assert_eq!(k.inlined_storage_size(), direct.inlined_storage_size());
        for (j, &i) in attrs.iter().enumerate() {
            prop_assert_eq!(k.column(j as u32).name(), s.column(i).name());
            prop_assert_eq!(k.column(j as u32).type_id(), s.column(i).type_id());
        }
        for (i, c) in s.columns().iter().enumerate() {
            let first = s.columns().iter().position(|d| d.name() == c.name()).unwrap() as u32;
            prop_assert_eq!(s.try_col_idx(c.name()), Some(first), "column {}", i);
        }
        prop_assert_eq!(s.try_col_idx("no such column"), None);
    }
}

// ---- 3b-02 · Tuples --------------------------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(config())]

    /// Whatever row goes into a tuple comes out of it, column by column, with its type: every type, every NULL, strings of any length.
    #[test]
    fn s3b_02_every_value_comes_back_from_its_tuple((cols, rows) in schema_and_rows()) {
        let s = schema_of(&cols);
        for bits in rows {
            let values = row(&cols, &bits);
            let t = Tuple::new(&values, &s);
            for (i, v) in values.iter().enumerate() {
                let got = t.get_value(&s, i as u32);
                prop_assert!(same(&got, v), "column {} of {}: put {:?}, got {:?}", i, s.to_string(true), v, got);
                prop_assert_eq!(t.is_null(&s, i as u32), v.is_null());
            }
        }
    }

    /// A tuple is as small as it can sensibly be: no more than the fixed part plus each text value's stored size, and no less than the fixed part.
    #[test]
    fn s3b_02_a_tuple_is_the_fixed_part_plus_its_text((cols, rows) in schema_and_rows()) {
        let s = schema_of(&cols);
        for bits in rows {
            let values = row(&cols, &bits);
            let t = Tuple::new(&values, &s);
            let texts: usize = s.uninlined_columns().iter().map(|&i| values[i as usize].storage_size()).sum();
            prop_assert!(t.get_length() as usize >= s.inlined_storage_size() as usize);
            prop_assert!(t.get_length() as usize <= s.inlined_storage_size() as usize + texts, "{} bytes for a fixed part of {} and texts of {}", t.get_length(), s.inlined_storage_size(), texts);
            prop_assert_eq!(t.data().len(), t.get_length() as usize);
            prop_assert!(!t.get_rid().page_id().is_valid(), "a new tuple is in no table");
        }
    }

    /// A key tuple holds the chosen columns, laid out by the key schema.
    #[test]
    fn s3b_02_a_key_tuple_has_the_chosen_columns((cols, rows) in schema_and_rows(), picks in prop::collection::vec(0usize..8, 1..4)) {
        let s = schema_of(&cols);
        let attrs: Vec<u32> = picks.iter().map(|p| (p % cols.len()) as u32).collect();
        let key_schema = Schema::copy_schema(&s, &attrs);
        let values = row(&cols, &rows[0]);
        let t = Tuple::new(&values, &s);
        let key = t.key_from_tuple(&s, &key_schema, &attrs);
        for (j, &i) in attrs.iter().enumerate() {
            prop_assert!(same(&key.get_value(&key_schema, j as u32), &values[i as usize]));
        }
    }

    /// A tuple serialised with its length prefix comes back byte for byte.
    #[test]
    fn s3b_02_a_tuple_serialises_with_a_length_prefix((cols, rows) in schema_and_rows(), offset in 0usize..8) {
        let s = schema_of(&cols);
        let t = Tuple::new(&row(&cols, &rows[0]), &s);
        let mut buf = vec![0xEEu8; offset + 4 + t.data().len() + 10];
        t.serialize_to(&mut buf[offset..]);
        prop_assert_eq!(&buf[offset..offset + 4], &(t.get_length() as i32).to_le_bytes(), "a 4-byte little-endian length first");
        prop_assert!(buf[offset + 4 + t.data().len()..].iter().all(|&b| b == 0xEE), "nothing written past the tuple");
        let back = Tuple::deserialize_from(&buf[offset..]);
        prop_assert_eq!(back.data(), t.data());
    }
}

#[test]
fn s3b_02_a_tuple_needs_one_value_per_column_of_the_right_type() {
    let s = Schema::new(vec![Column::new("a", Integer), Column::new_varchar("b", 10)]);
    assert!(std::panic::catch_unwind(|| Tuple::new(&[Value::integer(1)], &s)).is_err(), "too few values");
    assert!(std::panic::catch_unwind(|| Tuple::new(&[Value::integer(1), Value::varchar("x"), Value::integer(2)], &s)).is_err(), "too many values");
    assert!(std::panic::catch_unwind(|| Tuple::new(&[Value::varchar("x"), Value::integer(1)], &s)).is_err(), "values of the wrong types");
}

#[test]
fn s3b_02_tuples_print_and_can_be_made_from_bytes() {
    let s = Schema::new(vec![Column::new("a", Integer), Column::new_varchar("b", 10), Column::new("c", Decimal)]);
    let t = Tuple::new(&[Value::integer(4), Value::null(Varchar), Value::decimal(1.5)], &s);
    assert!(!t.is_null(&s, 0) && t.is_null(&s, 1));
    assert_eq!(t.to_string(&s), "(4, <NULL>, 1.500000)");
    assert_eq!(Tuple::new(&[Value::integer(-1), Value::varchar("x y"), Value::decimal(0.0)], &s).to_string(&s), "(-1, x y, 0.000000)");
    let copy = Tuple::from_bytes(Rid::new(PageId(3), 4), t.data());
    assert_eq!((copy.get_rid(), copy.data()), (Rid::new(PageId(3), 4), t.data()));
    assert_eq!(copy.to_string(&s), t.to_string(&s));
    assert!(!Tuple::empty().get_rid().page_id().is_valid());
}

// ---- 3b-03 · A table page: inserting ---------------------------------------------------------------------------------------

fn blob(n: u8, len: usize) -> Tuple {
    Tuple::from_bytes(Rid::default(), &vec![n; len])
}

#[test]
fn s3b_03_a_fresh_page_is_empty() {
    let mut bytes = [0xFFu8; BUSTUB_PAGE_SIZE]; // a recycled page full of garbage
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    assert_eq!((page.get_num_tuples(), page.get_num_deleted_tuples(), page.get_next_page_id()), (0, 0, None));
}

#[test]
fn s3b_03_tuples_get_consecutive_slot_numbers() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    for i in 0..20u16 {
        assert_eq!(page.insert_tuple(&meta(false), &blob(i as u8, 50)), Some(i));
        assert_eq!(page.get_num_tuples(), i as u32 + 1);
    }
}

#[test]
fn s3b_03_the_next_page_id_is_kept_and_can_be_cleared() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    page.set_next_page_id(Some(PageId(7)));
    page.insert_tuple(&meta(false), &blob(1, 10));
    assert_eq!(page.get_next_page_id(), Some(PageId(7)), "inserting does not disturb it");
    page.set_next_page_id(None);
    assert_eq!(page.get_next_page_id(), None);
}

#[test]
fn s3b_03_a_tuple_larger_than_the_page_is_refused_and_the_page_is_unchanged() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    page.insert_tuple(&meta(false), &blob(1, 100)).unwrap();
    assert!(page.get_next_tuple_offset(&meta(false), &blob(2, BUSTUB_PAGE_SIZE)).is_none());
    assert_eq!(page.insert_tuple(&meta(false), &blob(2, BUSTUB_PAGE_SIZE)), None);
    assert_eq!(page.get_num_tuples(), 1);
    // the biggest tuple an empty page can take is almost the whole page (the header and one slot are the only overhead)
    let mut other = [0u8; BUSTUB_PAGE_SIZE];
    let mut empty = TablePage::new(&mut other[..]);
    empty.init();
    let biggest = (BUSTUB_PAGE_SIZE - 200..BUSTUB_PAGE_SIZE).rev().find(|&len| empty.get_next_tuple_offset(&meta(false), &blob(0, len)).is_some());
    assert!(biggest.is_some_and(|b| b >= BUSTUB_PAGE_SIZE - 100), "an empty page should take a tuple of nearly {BUSTUB_PAGE_SIZE} bytes; the largest found was {biggest:?}");
}

proptest! {
    #![proptest_config(config())]

    /// Insert tuples of one size until the page is full: the count is within sensible bounds (the page wastes no more than a header and
    /// 32 bytes a tuple, and cannot hold more than its bytes), and `get_next_tuple_offset` says "no room" exactly when `insert_tuple` does.
    #[test]
    fn s3b_03_a_page_fills_up_to_sensible_bounds(len in 1usize..400) {
        let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
        let mut page = TablePage::new(&mut bytes[..]);
        page.init();
        let mut count = 0u32;
        loop {
            let t = blob(count as u8, len);
            let fits = page.get_next_tuple_offset(&meta(false), &t).is_some();
            let slot = page.insert_tuple(&meta(false), &t);
            prop_assert_eq!(fits, slot.is_some(), "offset and insert disagree at tuple {}", count);
            match slot {
                Some(s) => { prop_assert_eq!(s as u32, count); count += 1; }
                None => break,
            }
        }
        prop_assert_eq!(page.get_num_tuples(), count);
        let at_least = (BUSTUB_PAGE_SIZE - 64) / (len + 32);
        prop_assert!(count as usize >= at_least, "{} tuples of {} bytes fit; at least {} should", count, len, at_least);
        prop_assert!(count as usize <= BUSTUB_PAGE_SIZE / len, "{} tuples of {} bytes cannot fit in a page", count, len);
    }

    /// A page of mixed-size tuples keeps counting right and never overwrites: refused inserts leave the page as it was.
    #[test]
    fn s3b_03_a_refused_insert_changes_nothing(lens in prop::collection::vec(1usize..2000, 1..40)) {
        let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
        let mut page = TablePage::new(&mut bytes[..]);
        page.init();
        let mut stored = 0;
        for (i, &len) in lens.iter().enumerate() {
            match page.insert_tuple(&meta(false), &blob(i as u8, len)) {
                Some(slot) => { prop_assert_eq!(slot as u32, stored); stored += 1; }
                None => prop_assert_eq!(page.get_num_tuples(), stored),
            }
        }
        prop_assert_eq!(page.get_num_tuples(), stored);
    }
}

// ---- 3b-04 · A table page: reading and updating ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum PageOp {
    Insert(u8, usize),
    Get(usize),
    Delete(usize),
    Overwrite(usize, u8),
    Resize(usize),
}

fn page_ops() -> impl Strategy<Value = Vec<PageOp>> {
    prop::collection::vec(
        prop_oneof![
            4 => (any::<u8>(), 1usize..120).prop_map(|(b, l)| PageOp::Insert(b, l)),
            3 => (0usize..40).prop_map(PageOp::Get),
            2 => (0usize..40).prop_map(PageOp::Delete),
            2 => (0usize..40, any::<u8>()).prop_map(|(i, b)| PageOp::Overwrite(i, b)),
            1 => (0usize..40).prop_map(PageOp::Resize),
        ],
        1..120,
    )
}

proptest! {
    #![proptest_config(config())]

    /// A page against a `Vec` of `(meta, bytes)`: any mix of inserts, reads, deletes and in-place overwrites. A slot past the last tuple is an
    /// error and not a panic; a deleted tuple keeps its bytes; an overwrite of another length is refused and changes nothing; the deleted
    /// count goes up each time a live tuple is marked deleted.
    #[test]
    fn s3b_04_a_page_behaves_like_a_vec_of_tuples(ops in page_ops()) {
        let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
        let mut page = TablePage::new(&mut bytes[..]);
        page.init();
        let mut model: Vec<(TupleMeta, Vec<u8>)> = Vec::new();
        let mut newly_deleted = 0usize; // each time a live tuple becomes deleted (a later un-delete does not take it back)
        for op in ops {
            match op {
                PageOp::Insert(b, len) => {
                    if let Some(slot) = page.insert_tuple(&meta(false), &blob(b, len)) {
                        prop_assert_eq!(slot as usize, model.len());
                        model.push((meta(false), vec![b; len]));
                    }
                }
                PageOp::Get(i) => {
                    let rid = Rid::new(PageId(5), i as u32);
                    match (page.get_tuple(rid), model.get(i)) {
                        (Ok((m, t)), Some((mm, data))) => {
                            prop_assert_eq!(m, *mm);
                            prop_assert_eq!(t.data(), &data[..]);
                            prop_assert_eq!(t.get_rid(), rid, "the tuple knows its record id");
                            prop_assert_eq!(page.get_tuple_meta(rid).unwrap(), *mm);
                        }
                        (Err(e), None) => { prop_assert!(page.get_tuple_meta(rid).is_err()); let _ = e; }
                        (got, want) => prop_assert!(false, "slot {}: page says {:?}, model says {:?}", i, got.map(|x| x.0), want),
                    }
                }
                PageOp::Delete(i) => {
                    let rid = Rid::new(PageId(5), i as u32);
                    let r = page.update_tuple_meta(&meta(true), rid);
                    match model.get_mut(i) {
                        Some(entry) => { prop_assert!(r.is_ok()); if !entry.0.is_deleted { newly_deleted += 1; } entry.0 = meta(true); }
                        None => prop_assert!(r.is_err()),
                    }
                }
                PageOp::Overwrite(i, b) => {
                    let rid = Rid::new(PageId(5), i as u32);
                    let len = model.get(i).map(|e| e.1.len()).unwrap_or(10);
                    let r = page.update_tuple_in_place_unsafe(&TupleMeta { ts: 9, is_deleted: false }, &blob(b, len), rid);
                    match model.get_mut(i) {
                        Some(entry) => { prop_assert!(r.is_ok()); *entry = (TupleMeta { ts: 9, is_deleted: false }, vec![b; len]); } // revives a deleted tuple
                        None => prop_assert!(r.is_err()),
                    }
                }
                PageOp::Resize(i) => {
                    let rid = Rid::new(PageId(5), i as u32);
                    if let Some(entry) = model.get(i) {
                        prop_assert!(page.update_tuple_in_place_unsafe(&meta(false), &blob(1, entry.1.len() + 1), rid).is_err(), "another length is refused");
                    }
                }
            }
            prop_assert_eq!(page.get_num_tuples() as usize, model.len());
            prop_assert_eq!(page.get_num_deleted_tuples() as usize, newly_deleted, "the deleted count goes up each time a live tuple is marked deleted");
        }
    }
}

#[test]
fn s3b_04_marking_deleted_counts_once_and_keeps_the_bytes() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    let rid = Rid::new(PageId(1), page.insert_tuple(&meta(false), &blob(7, 30)).unwrap() as u32);
    page.update_tuple_meta(&meta(true), rid).unwrap();
    page.update_tuple_meta(&meta(true), rid).unwrap();
    assert_eq!(page.get_num_deleted_tuples(), 1, "a tuple is counted once however often it is marked");
    let (m, t) = page.get_tuple(rid).unwrap();
    assert!(m.is_deleted && t.data() == &[7u8; 30][..]);
}

#[test]
fn s3b_04_marking_deleted_by_an_in_place_update_counts_too() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    let rid = Rid::new(PageId(1), page.insert_tuple(&meta(false), &blob(7, 30)).unwrap() as u32);
    page.update_tuple_in_place_unsafe(&meta(true), &blob(8, 30), rid).unwrap();
    assert_eq!(page.get_num_deleted_tuples(), 1, "a live tuple that becomes deleted through an in-place update is counted");
    page.update_tuple_in_place_unsafe(&meta(true), &blob(9, 30), rid).unwrap();
    assert_eq!(page.get_num_deleted_tuples(), 1, "an already deleted tuple is not counted again, however it is updated");
    let (m, t) = page.get_tuple(rid).unwrap();
    assert!(m.is_deleted && t.data() == &[9u8; 30][..], "the meta and the bytes of the last update are what is stored");
}

#[test]
fn s3b_04_a_page_is_nothing_but_its_bytes() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    {
        let mut page = TablePage::new(&mut bytes[..]);
        page.init();
        page.set_next_page_id(Some(PageId(3)));
        for i in 0..30u8 {
            page.insert_tuple(&TupleMeta { ts: i as i64, is_deleted: i % 4 == 0 }, &blob(i, 20 + i as usize)).unwrap();
        }
    }
    let copy = bytes; // the page's bytes copied somewhere else, as the buffer pool does when it writes and reads a page
    let page = TablePage::new(&copy[..]);
    assert_eq!((page.get_num_tuples(), page.get_next_page_id()), (30, Some(PageId(3))));
    for i in 0..30u8 {
        let (m, t) = page.get_tuple(Rid::new(PageId(0), i as u32)).unwrap();
        assert_eq!((m.ts, m.is_deleted, t.data()), (i as i64, i % 4 == 0, &vec![i; 20 + i as usize][..]));
    }
}

#[test]
fn s3b_04_changing_the_metadata_keeps_the_bytes_and_a_same_length_overwrite_keeps_the_neighbours() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    let rids: Vec<Rid> = (0..5u8).map(|i| Rid::new(PageId(0), page.insert_tuple(&meta(false), &blob(i, 30)).unwrap() as u32)).collect();
    page.update_tuple_meta(&TupleMeta { ts: 77, is_deleted: false }, rids[2]).unwrap();
    assert_eq!(page.get_tuple(rids[2]).unwrap().1.data(), &[2u8; 30][..], "new metadata, same bytes");
    page.update_tuple_in_place_unsafe(&meta(false), &blob(9, 30), rids[2]).unwrap();
    for (i, rid) in rids.iter().enumerate() {
        let want = if i == 2 { 9 } else { i as u8 };
        assert_eq!(page.get_tuple(*rid).unwrap().1.data(), &[want; 30][..], "slot {i}");
    }
}

#[test]
fn s3b_04_errors_from_a_bad_slot_have_a_kind() {
    let mut bytes = [0u8; BUSTUB_PAGE_SIZE];
    let mut page = TablePage::new(&mut bytes[..]);
    page.init();
    let e = page.get_tuple(Rid::new(PageId(0), 0)).unwrap_err();
    assert_eq!(e.kind, ExceptionType::Invalid);
}

// ---- 3b-05 · Boss: everything together ------------------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..config() })]

    /// A table is a chain of pages: fill pages with random tuples (starting another when one is full), copy every page's bytes, follow the
    /// chain, and read every row back by its record id.
    #[test]
    fn s3b_05_rows_survive_a_chain_of_pages((cols, rows) in schema_and_rows(), copies in 20usize..80) {
        let s = schema_of(&cols);
        let mut pages: Vec<[u8; BUSTUB_PAGE_SIZE]> = vec![[0; BUSTUB_PAGE_SIZE]];
        TablePage::new(&mut pages[0][..]).init();
        let mut stored: Vec<(Rid, Vec<Value>)> = vec![];
        for n in 0..copies {
            let values = row(&cols, &rows[n % rows.len()]);
            let tuple = Tuple::new(&values, &s);
            let last = pages.len() - 1;
            let (page_no, slot) = match TablePage::new(&mut pages[last][..]).insert_tuple(&meta(false), &tuple) {
                Some(slot) => (last, slot),
                None => {
                    pages.push([0; BUSTUB_PAGE_SIZE]);
                    TablePage::new(&mut pages[last + 1][..]).init();
                    TablePage::new(&mut pages[last][..]).set_next_page_id(Some(PageId(last as i32 + 1)));
                    (last + 1, TablePage::new(&mut pages[last + 1][..]).insert_tuple(&meta(false), &tuple).expect("a tuple that fits an empty page"))
                }
            };
            stored.push((Rid::new(PageId(page_no as i32), slot as u32), values));
        }
        let copied = pages.clone();
        let (mut at, mut total) = (Some(PageId(0)), 0);
        while let Some(id) = at {
            let page = TablePage::new(&copied[id.0 as usize][..]);
            total += page.get_num_tuples() as usize;
            at = page.get_next_page_id();
        }
        prop_assert_eq!(total, copies, "following the chain finds every tuple once");
        for (rid, values) in stored {
            let (_, t) = TablePage::new(&copied[rid.page_id().0 as usize][..]).get_tuple(rid).unwrap();
            for (i, v) in values.iter().enumerate() {
                prop_assert!(same(&t.get_value(&s, i as u32), v));
            }
        }
    }
}

#[test]
fn s3b_05_comparing_values_read_from_a_tuple_works_like_comparing_the_originals() {
    let s = Schema::new(vec![Column::new("a", Integer), Column::new_varchar("b", 10)]);
    let t = Tuple::new(&[Value::integer(7), Value::varchar("7")], &s);
    assert_eq!(t.get_value(&s, 0).compare_equals(&t.get_value(&s, 1)).unwrap(), CmpBool::True, "7 = '7' even after a trip through bytes");
}

// @@ challenge 3b-c1 begin
mod ch_3b_c1 {
    use proptest::prelude::*;

    use bustub::storage::table::null_row::{decode_row, encode_row, Cell, Ty};

    fn i(v: i64) -> Option<Cell> {
        Some(Cell::Int(v))
    }
    fn t(s: &str) -> Option<Cell> {
        Some(Cell::Text(s.to_owned()))
    }

    #[test]
    fn s3b_c1_the_exact_bytes_of_a_mixed_row() {
        let bytes = encode_row(&[i(1), None, t("ab")]);
        let mut want = vec![0b010];
        want.extend_from_slice(&1i64.to_le_bytes());
        want.extend_from_slice(&2u32.to_le_bytes());
        want.extend_from_slice(b"ab");
        assert_eq!(bytes, want);
        assert_eq!(encode_row(&[None, None]), vec![0b11]);
    }

    #[test]
    fn s3b_c1_rows_round_trip_including_more_than_eight_columns() {
        let row: Vec<Option<Cell>> = (0..11).map(|n| if n % 3 == 0 { None } else if n % 2 == 0 { t("x") } else { i(n) }).collect();
        let types: Vec<Ty> = row.iter().enumerate().map(|(n, _)| if n % 2 == 0 { Ty::Text } else { Ty::Int }).collect();
        assert_eq!(decode_row(&encode_row(&row), &types), Some(row));
    }

    #[test]
    fn s3b_c1_a_null_costs_one_bit_not_a_value() {
        let full = encode_row(&[i(1), i(2), i(3)]);
        let nulls = encode_row(&[None, None, None]);
        assert_eq!((full.len(), nulls.len()), (1 + 24, 1));
    }

    #[test]
    fn s3b_c1_malformed_bytes_are_rejected() {
        let types = [Ty::Int, Ty::Text];
        let good = encode_row(&[i(7), t("hi")]);
        for cut in 0..good.len() {
            assert_eq!(decode_row(&good[..cut], &types), None, "cut at {cut}");
        }
        let mut trailing = good.clone();
        trailing.push(0);
        assert_eq!(decode_row(&trailing, &types), None);
        let mut bad_utf8 = vec![0u8, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0xFF];
        bad_utf8[9] = 1;
        assert_eq!(decode_row(&bad_utf8, &types), None);
        assert_eq!(decode_row(&[0b100], &types), None, "a bit set past the last column");
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: round trip, the size formula, and no panic on arbitrary bytes.
        #[test]
        fn s3b_c1_property_rows_round_trip(cols in proptest::collection::vec(proptest::option::of(prop_oneof![any::<i64>().prop_map(Cell::Int), "[a-z]{0,6}".prop_map(Cell::Text)]), 0..14)) {
            let types: Vec<Ty> = cols.iter().map(|c| match c { Some(Cell::Text(_)) => Ty::Text, _ => Ty::Int }).collect();
            // a NULL column may be either type: use Int for it
            let bytes = encode_row(&cols);
            let want_len = cols.len().div_ceil(8) + cols.iter().flatten().map(|c| match c { Cell::Int(_) => 8, Cell::Text(s) => 4 + s.len() }).sum::<usize>();
            prop_assert_eq!(bytes.len(), want_len);
            prop_assert_eq!(decode_row(&bytes, &types), Some(cols));
        }

        #[test]
        fn s3b_c1_property_arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..40), types in proptest::collection::vec(prop_oneof![Just(Ty::Int), Just(Ty::Text)], 0..6)) {
            if let Some(row) = decode_row(&bytes, &types) {
                prop_assert_eq!(encode_row(&row), bytes, "accepted bytes must be exactly what encoding the row gives");
            }
        }
    }
}
// @@ challenge 3b-c1 end

// @@ challenge 3b-c2 begin
mod ch_3b_c2 {
    use proptest::prelude::*;

    use bustub::storage::table::struct_layout::{layout, packed_order, Layout};

    #[test]
    fn s3b_c2_padding_between_and_after_fields() {
        let l = layout(&[(1, 1), (8, 8), (2, 2)]);
        assert_eq!(l, Layout { offsets: vec![0, 8, 16], size: 24, align: 8 });
    }

    #[test]
    fn s3b_c2_no_fields_and_one_field() {
        assert_eq!(layout(&[]), Layout { offsets: vec![], size: 0, align: 1 });
        assert_eq!(layout(&[(4, 4)]), Layout { offsets: vec![0], size: 4, align: 4 });
    }

    #[test]
    fn s3b_c2_the_packed_order_removes_the_padding() {
        let fields = [(1, 1), (8, 8), (2, 2)];
        let order = packed_order(&fields);
        assert_eq!(order, vec![1, 2, 0]);
        let packed: Vec<_> = order.iter().map(|&i| fields[i]).collect();
        assert_eq!(layout(&packed).size, 16);
    }

    #[test]
    fn s3b_c2_equal_alignments_keep_their_order() {
        assert_eq!(packed_order(&[(4, 4), (4, 4), (4, 4)]), vec![0, 1, 2]);
    }

    fn permutations(n: usize) -> Vec<Vec<usize>> {
        if n == 0 {
            return vec![vec![]];
        }
        let mut out = Vec::new();
        for p in permutations(n - 1) {
            for at in 0..=p.len() {
                let mut q = p.clone();
                q.insert(at, n - 1);
                out.push(q);
            }
        }
        out
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: layouts are valid, and the packed order is as small as the best of all permutations.
        #[test]
        fn s3b_c2_property_layouts_are_valid_and_the_packed_order_is_minimal(fields in proptest::collection::vec((0u32..4, 1usize..4), 0..6)) {
            let fields: Vec<(usize, usize)> = fields.into_iter().map(|(mult, e)| { let align = 1usize << (e - 1); (align * (mult as usize), align) }).collect();
            let l = layout(&fields);
            for (i, &(size, a)) in fields.iter().enumerate() {
                prop_assert_eq!(l.offsets[i] % a, 0);
                if i > 0 { prop_assert!(l.offsets[i] >= l.offsets[i - 1] + fields[i - 1].0); }
                let _ = size;
            }
            prop_assert_eq!(l.size % l.align, 0);
            prop_assert!(l.size >= fields.iter().map(|f| f.0).sum::<usize>());
            let best = permutations(fields.len()).into_iter().map(|p| layout(&p.iter().map(|&i| fields[i]).collect::<Vec<_>>()).size).min().unwrap();
            let order = packed_order(&fields);
            let mut sorted = order.clone();
            sorted.sort();
            prop_assert_eq!(sorted, (0..fields.len()).collect::<Vec<_>>());
            prop_assert_eq!(layout(&order.iter().map(|&i| fields[i]).collect::<Vec<_>>()).size, best);
        }
    }
}
// @@ challenge 3b-c2 end

// @@ challenge 3b-c3 begin
mod ch_3b_c3 {
    use proptest::prelude::*;

    use bustub::storage::table::var_row::{column, encode};

    fn cols(v: &[&str]) -> Vec<Vec<u8>> {
        v.iter().map(|s| s.as_bytes().to_vec()).collect()
    }

    #[test]
    fn s3b_c3_each_column_comes_back() {
        let b = encode(&cols(&["ab", "", "cde"]));
        assert_eq!(column(&b, 0), Some(&b"ab"[..]));
        assert_eq!(column(&b, 1), Some(&b""[..]));
        assert_eq!(column(&b, 2), Some(&b"cde"[..]));
    }

    #[test]
    fn s3b_c3_an_index_past_the_end_is_none() {
        let b = encode(&cols(&["x"]));
        assert_eq!(column(&b, 1), None);
        assert_eq!(column(&[], 0), None);
    }

    #[test]
    fn s3b_c3_the_first_column_starts_at_zero_and_a_single_column_row_works() {
        let b = encode(&cols(&["hello"]));
        assert_eq!(column(&b, 0), Some(&b"hello"[..]));
    }

    #[test]
    fn s3b_c3_empty_columns_at_the_end_and_a_row_of_only_empty_columns() {
        let b = encode(&cols(&["x", "", ""]));
        assert_eq!((column(&b, 0), column(&b, 1), column(&b, 2)), (Some(&b"x"[..]), Some(&b""[..]), Some(&b""[..])));
        let e = encode(&cols(&["", ""]));
        assert_eq!((column(&e, 0), column(&e, 1), column(&e, 2)), (Some(&b""[..]), Some(&b""[..]), None));
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: every column reads back, and together they are exactly the data section.
        #[test]
        fn s3b_c3_property_columns_read_back(rows in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..6), 0..8)) {
            let b = encode(&rows);
            let mut all = Vec::new();
            for (i, c) in rows.iter().enumerate() {
                prop_assert_eq!(column(&b, i), Some(c.as_slice()), "column {}", i);
                all.extend_from_slice(column(&b, i).unwrap());
            }
            prop_assert_eq!(all.len(), rows.iter().map(|c| c.len()).sum::<usize>());
            prop_assert_eq!(column(&b, rows.len()), None);
        }
    }
}
// @@ challenge 3b-c3 end

// @@ challenge 3b-c4 begin
mod ch_3b_c4 {
    use proptest::prelude::*;

    use bustub::storage::table::checksum::{crc32, seal, unseal, SealError};

    #[test]
    fn s3b_c4_the_standard_check_values() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"The quick brown fox jumps over the lazy dog"), 0x414F_A339);
    }

    #[test]
    fn s3b_c4_sealing_appends_the_crc_in_little_endian() {
        let s = seal(b"abc");
        assert_eq!(&s[..3], b"abc");
        assert_eq!(&s[3..], &crc32(b"abc").to_le_bytes());
        assert_eq!(unseal(&s), Ok(&b"abc"[..]));
        assert_eq!(unseal(&seal(b"")), Ok(&b""[..]));
    }

    #[test]
    fn s3b_c4_short_and_damaged_buffers_are_refused() {
        assert_eq!(unseal(b"abc"), Err(SealError::TooShort));
        assert_eq!(unseal(b""), Err(SealError::TooShort));
        let mut s = seal(b"hello world");
        s[2] ^= 1;
        assert_eq!(unseal(&s), Err(SealError::Mismatch));
    }

    #[test]
    fn s3b_c4_every_single_bit_flip_of_a_sealed_page_is_detected() {
        let page: Vec<u8> = (0..200u32).map(|i| (i * 31 % 251) as u8).collect();
        let s = seal(&page);
        for byte in 0..s.len() {
            for bit in 0..8 {
                let mut t = s.clone();
                t[byte] ^= 1 << bit;
                assert!(unseal(&t).is_err(), "flipping bit {bit} of byte {byte} went unnoticed");
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: seal then unseal is the identity, and dropping or adding a byte is caught.
        #[test]
        fn s3b_c4_property_sealed_payloads_round_trip_and_changes_are_caught(p in proptest::collection::vec(any::<u8>(), 0..64), extra in any::<u8>()) {
            let s = seal(&p);
            prop_assert_eq!(unseal(&s), Ok(p.as_slice()));
            let mut more = s.clone();
            more.push(extra);
            prop_assert!(unseal(&more).is_err());
            if !p.is_empty() {
                let mut less = s.clone();
                less.remove(0);
                prop_assert!(unseal(&less).is_err());
            }
        }
    }
}
// @@ challenge 3b-c4 end

// @@ challenge 3b-c5 begin
mod ch_3b_c5 {
    use proptest::prelude::*;

    use bustub::storage::table::distinct::{distinct_count, distinct_rows, Row};

    fn r(v: &[Option<i64>]) -> Row {
        v.to_vec()
    }

    #[test]
    fn s3b_c5_nulls_are_the_same_for_distinct() {
        let rows = vec![r(&[Some(1), None]), r(&[Some(1), None]), r(&[Some(2), Some(3)]), r(&[Some(1), None])];
        assert_eq!(distinct_rows(&rows), vec![r(&[Some(1), None]), r(&[Some(2), Some(3)])]);
        assert_eq!(distinct_count(&rows), 2);
    }

    #[test]
    fn s3b_c5_null_and_zero_are_different() {
        let rows = vec![r(&[None]), r(&[Some(0)]), r(&[None])];
        assert_eq!(distinct_rows(&rows), vec![r(&[None]), r(&[Some(0)])]);
    }

    #[test]
    fn s3b_c5_first_occurrences_keep_their_order() {
        let rows = vec![r(&[Some(3)]), r(&[Some(1)]), r(&[Some(3)]), r(&[Some(2)]), r(&[Some(1)])];
        assert_eq!(distinct_rows(&rows), vec![r(&[Some(3)]), r(&[Some(1)]), r(&[Some(2)])]);
    }

    #[test]
    fn s3b_c5_empty_input_and_rows_of_different_lengths() {
        assert_eq!(distinct_rows(&[]), Vec::<Row>::new());
        assert_eq!(distinct_rows(&[r(&[]), r(&[]), r(&[None])]), vec![r(&[]), r(&[None])]);
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 256, failure_persistence: None, ..ProptestConfig::default() })]

        /// Property: a first-seen filter, idempotent, with a count that does not depend on order.
        #[test]
        fn s3b_c5_property_distinct_keeps_first_occurrences(rows in proptest::collection::vec(proptest::collection::vec(proptest::option::of(0i64..3), 0..3), 0..20)) {
            let out = distinct_rows(&rows);
            let mut want: Vec<Row> = Vec::new();
            for row in &rows { if !want.contains(row) { want.push(row.clone()); } }
            prop_assert_eq!(&out, &want);
            prop_assert_eq!(distinct_rows(&out), out.clone());
            prop_assert_eq!(distinct_count(&rows), out.len());
            let mut rev = rows.clone();
            rev.reverse();
            prop_assert_eq!(distinct_count(&rev), out.len());
        }
    }
}
// @@ challenge 3b-c5 end
