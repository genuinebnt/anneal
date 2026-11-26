use solution::*;

/// A database with the given committed values.
#[allow(dead_code)]
fn with(pairs: &[(&str, i64)]) -> Db {
    let mut db = Db::new();
    let t = db.begin();
    for &(k, v) in pairs {
        db.put(t, k, v).unwrap();
    }
    db.commit(t).unwrap();
    db
}

#[test]
fn own_writes_are_visible_and_private() {
    let mut db = Db::new();
    let (a, b) = (db.begin(), db.begin());
    db.put(a, "x", 1).unwrap();
    check!("a reads its write, b does not", (db.get(a, "x"), db.get(b, "x")), (Ok(Some(1)), Ok(None)));
}

#[test]
fn snapshot_does_not_move() {
    let mut db = with(&[("x", 1)]);
    let reader = db.begin();
    let writer = db.begin();
    db.put(writer, "x", 2).unwrap();
    db.commit(writer).unwrap();
    let late = db.begin();
    check!("(older snapshot, newer one)", (db.get(reader, "x"), db.get(late, "x")), (Ok(Some(1)), Ok(Some(2))));
}

#[test]
fn first_committer_wins() {
    let mut db = with(&[("x", 1)]);
    let (a, b) = (db.begin(), db.begin());
    db.put(a, "x", 10).unwrap();
    db.put(b, "x", 20).unwrap();
    db.put(b, "y", 5).unwrap();
    check!("a commits, then b", (db.commit(a), db.commit(b)), (Ok(()), Err(TxError::Conflict)));
    let r = db.begin();
    check!("nothing of b was applied", (db.get(r, "x"), db.get(r, "y")), (Ok(Some(10)), Ok(None)));
    check!("b is over", db.get(b, "x"), Err(TxError::NotActive));
}

#[test]
fn write_skew_is_allowed() {
    // Two doctors on call; each checks the other and goes home.
    let mut db = with(&[("alice", 1), ("bob", 1)]);
    let (a, b) = (db.begin(), db.begin());
    db.get(a, "bob").unwrap();
    db.get(b, "alice").unwrap();
    db.put(a, "alice", 0).unwrap();
    db.put(b, "bob", 0).unwrap();
    check!("disjoint writes both commit", (db.commit(a), db.commit(b)), (Ok(()), Ok(())));
}

#[test]
fn vacuum_drops_hidden_versions() {
    let mut db = with(&[("x", 1)]);
    for v in 2..=5 {
        let t = db.begin();
        db.put(t, "x", v).unwrap();
        db.commit(t).unwrap();
    }
    check!("(versions, removed, versions)", (db.versions(), db.vacuum(), db.versions()), (5, 4, 1));
}
