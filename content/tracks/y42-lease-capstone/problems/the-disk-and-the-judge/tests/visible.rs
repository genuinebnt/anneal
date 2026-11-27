use solution::*;

/// A write by "the client with the same number as the token".
#[allow(dead_code)]
fn w(token: u64, value: u64) -> Write {
    Write {
        time: value,
        client: token as u32,
        token,
        value,
    }
}

#[allow(dead_code)]
fn judged(disk: &Disk) -> Result<(), Breach> {
    judge(disk.arrived(), disk.accepted(), disk.value())
}

#[test]
fn writes_in_token_order_are_fine() {
    let mut disk = Disk::new(false);
    check!("empty", disk.value(), None);
    for write in [w(1, 1), w(1, 2), w(2, 3)] {
        check!("accepted", disk.write(write), true);
    }
    check!("value", disk.value(), Some(3));
    check!("verdict", judged(&disk), Ok(()));
}

#[test]
fn an_unguarded_disk_takes_a_stale_write() {
    let mut disk = Disk::new(false);
    disk.write(w(2, 10));
    check!("taken", disk.write(w(1, 11)), true);
    check!("value", disk.value(), Some(11));
    check!("verdict", judged(&disk), Err(Breach::StaleWrite { newer: w(2, 10), stale: w(1, 11) }));
}

#[test]
fn a_guarded_disk_turns_it_away() {
    let mut disk = Disk::new(true);
    disk.write(w(2, 10));
    check!("turned away", disk.write(w(1, 11)), false);
    check!("value", disk.value(), Some(10));
    check!("it did arrive", disk.arrived().len(), 2);
    check!("but was not accepted", disk.accepted().len(), 1);
    check!("verdict", judged(&disk), Ok(()));
}

#[test]
fn the_same_holder_writes_again() {
    let mut disk = Disk::new(true);
    disk.write(w(3, 1));
    check!("same token", disk.write(w(3, 2)), true);
}

#[test]
fn a_disk_that_loses_the_newest_write() {
    // the broken disk accepted both, then shows the older value
    let writes = [w(1, 1), w(2, 2)];
    check!("lost", judge(&writes, &writes, Some(1)), Err(Breach::NewestLost { newest: w(2, 2), found: Some(1) }));
}

#[test]
fn nothing_happened() {
    check!("empty", judge(&[], &[], None), Ok(()));
}
