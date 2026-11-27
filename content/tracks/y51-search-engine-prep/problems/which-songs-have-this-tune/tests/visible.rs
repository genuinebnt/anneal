use solution::*;

fn book(songs: &[&[u32]]) -> Songbook {
    let mut b = Songbook::new();
    for s in songs {
        b.add(s);
    }
    b
}

#[test]
fn songs_are_numbered() {
    let mut b = Songbook::new();
    check!("first and second", (b.add(&[1]), b.add(&[])), (0, 1));
}

#[test]
fn a_tune_in_two_songs() {
    let b = book(&[&[60, 62, 64, 65], &[50, 62, 64], &[64, 62]]);
    check!("62 64", b.find(&[62, 64]), vec![(0, 1), (1, 1)]);
}

#[test]
fn the_notes_must_touch_and_keep_their_order() {
    let b = book(&[&[1, 9, 2], &[2, 1], &[1, 2]]);
    check!("1 2", b.find(&[1, 2]), vec![(2, 0)]);
}

#[test]
fn every_start_is_reported() {
    let b = book(&[&[7, 7, 7], &[7, 8, 7, 8, 7]]);
    check!("7 7", b.find(&[7, 7]), vec![(0, 0), (0, 1)]);
    check!("7 8 7", b.find(&[7, 8, 7]), vec![(1, 0), (1, 2)]);
}

#[test]
fn a_tune_stays_inside_one_song() {
    let b = book(&[&[1, 2], &[3, 4]]);
    check!("2 3", b.find(&[2, 3]), Vec::new());
}

#[test]
fn nothing_to_find() {
    let b = book(&[&[1, 2, 3]]);
    check!("empty tune", b.find(&[]), Vec::new());
    check!("unknown note", b.find(&[1, 99]), Vec::new());
    check!("longer than the song", b.find(&[1, 2, 3, 1]), Vec::new());
}
