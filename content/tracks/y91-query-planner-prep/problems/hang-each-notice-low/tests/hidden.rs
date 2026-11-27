use solution::*;
use anneal_prelude::Rng;

#[allow(dead_code)]
fn room(n: u8) -> Place {
    Place::Room(n)
}
#[allow(dead_code)]
fn wing(a: Place, b: Place) -> Place {
    Place::Wing(Box::new(a), Box::new(b))
}

#[allow(dead_code)]
fn rooms_of(p: &Place, out: &mut Vec<u8>) {
    match p {
        Place::Room(n) => out.push(*n),
        Place::Wing(a, b) => {
            rooms_of(a, out);
            rooms_of(b, out);
        }
    }
}

/// The slow way: of all places that contain every room of the notice, the deepest.
#[allow(dead_code)]
fn slow(p: &Place, notice: &[u8], path: &mut Vec<Turn>, best: &mut Option<Vec<Turn>>) {
    let mut here = Vec::new();
    rooms_of(p, &mut here);
    let deeper = best.as_ref().is_none_or(|b| path.len() > b.len());
    if deeper && notice.iter().all(|r| here.contains(r)) {
        *best = Some(path.clone());
    }
    if let Place::Wing(a, b) = p {
        for (turn, child) in [(Turn::Left, a), (Turn::Right, b)] {
            path.push(turn);
            slow(child, notice, path, best);
            path.pop();
        }
    }
}

#[allow(dead_code)]
fn build(rng: &mut Rng, rooms: &[u8]) -> Place {
    if rooms.len() == 1 {
        return room(rooms[0]);
    }
    let cut = 1 + rng.below(rooms.len() - 1);
    wing(build(rng, &rooms[..cut]), build(rng, &rooms[cut..]))
}

#[test]
fn random_buildings() {
    let mut rng = Rng::new(9141);
    for case in 0..600 {
        let mut ids: Vec<u8> = (0..64).collect();
        rng.shuffle(&mut ids);
        ids.truncate(1 + rng.below(if case % 20 == 0 { 64 } else { 9 }));
        let building = build(&mut rng, &ids);
        let notices: Vec<Vec<u8>> = (0..8)
            .map(|_| {
                let n = 1 + rng.below(4);
                (0..n)
                    .map(|_| match rng.below(12) {
                        0 => rng.int(0, 255) as u8,
                        _ => *rng.pick(&ids),
                    })
                    .collect()
            })
            .collect();
        let want: Vec<Option<Vec<Turn>>> = notices
            .iter()
            .map(|n| {
                let mut best = None;
                slow(&building, n, &mut Vec::new(), &mut best);
                best
            })
            .collect();
        check!(format!("case {case}: {building:?}, notices {notices:?}"), hang(&building, &notices), want);
    }
}

#[test]
fn room_numbers_the_building_cannot_have() {
    let b = wing(room(0), room(63));
    let notices = vec![vec![64], vec![0, 128], vec![255], vec![63], vec![62]];
    check!("rooms 64, 128 and 255", hang(&b, &notices), vec![None, None, None, Some(vec![Turn::Right]), None]);
}

#[test]
fn no_notices() {
    check!("nothing to hang", hang(&room(0), &[]), Vec::<Option<Vec<Turn>>>::new());
}

#[test]
fn a_long_corridor_and_many_notices() {
    let mut building = room(63);
    for n in (0..63).rev() {
        building = wing(room(n), building);
    }
    let notices: Vec<Vec<u8>> = (0..20_000).map(|i| vec![(i % 64) as u8, 63]).collect();
    let got = hang(&building, &notices);
    let lengths_ok = got.iter().enumerate().all(|(i, w)| w.as_ref().is_some_and(|w| w.len() == (i % 64)));
    check!("20 000 notices in a corridor of 64 rooms", lengths_ok, true);
}

#[test]
fn every_room_alone_and_all_together() {
    let b = wing(wing(room(5), wing(room(9), room(1))), room(30));
    use Turn::{Left, Right};
    let notices = vec![vec![5], vec![9], vec![1], vec![30], vec![5, 9, 1, 30], vec![5, 9, 1]];
    check!("each room, everyone, and the left part", hang(&b, &notices), vec![
        Some(vec![Left, Left]),
        Some(vec![Left, Right, Left]),
        Some(vec![Left, Right, Right]),
        Some(vec![Right]),
        Some(vec![]),
        Some(vec![Left]),
    ]);
}

#[test]
fn one_unknown_room_among_known_ones() {
    let b = wing(room(0), room(2));
    check!("room 1 is missing", hang(&b, &[vec![0, 1, 2], vec![0, 2]]), vec![None, Some(vec![])]);
}

#[test]
fn sixty_four_rooms_in_a_balanced_building() {
    fn build(from: u8, to: u8) -> Place {
        if to - from == 1 {
            return room(from);
        }
        let mid = (from + to) / 2;
        wing(build(from, mid), build(mid, to))
    }
    let got = hang(&build(0, 64), &[vec![0, 63], vec![32, 63], vec![63]]);
    check!("depths", got.iter().map(|w| w.as_ref().map(Vec::len)).collect::<Vec<_>>(), vec![Some(0), Some(1), Some(6)]);
}

#[test]
fn notices_keep_their_order() {
    let b = wing(room(0), room(1));
    use Turn::{Left, Right};
    check!("right, left, right", hang(&b, &[vec![1], vec![0], vec![1]]), vec![Some(vec![Right]), Some(vec![Left]), Some(vec![Right])]);
}
