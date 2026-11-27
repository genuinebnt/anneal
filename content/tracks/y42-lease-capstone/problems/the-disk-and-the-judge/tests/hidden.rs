use solution::*;

#[allow(dead_code)]
fn w(token: u64, value: u64) -> Write {
    Write {
        time: value,
        client: token as u32,
        token,
        value,
    }
}

#[test]
fn the_first_stale_write_is_reported() {
    let accepted = [w(1, 1), w(5, 2), w(3, 3), w(2, 4)];
    check!("first, not last", judge(&accepted, &accepted, Some(4)), Err(Breach::StaleWrite { newer: w(5, 2), stale: w(3, 3) }));
}

#[test]
fn newer_is_the_write_just_before() {
    let accepted = [w(4, 1), w(4, 2), w(1, 3)];
    check!("the latest of token 4", judge(&accepted, &accepted, Some(3)), Err(Breach::StaleWrite { newer: w(4, 2), stale: w(1, 3) }));
}

#[test]
fn the_stale_rule_comes_first() {
    // the end value is wrong too
    let accepted = [w(2, 1), w(1, 2)];
    check!("rule 1", judge(&accepted, &accepted, Some(99)), Err(Breach::StaleWrite { newer: w(2, 1), stale: w(1, 2) }));
}

#[test]
fn a_newest_write_that_was_turned_away_is_lost() {
    // a broken guard rejected the newest write
    let arrived = [w(1, 1), w(2, 2)];
    check!("lost", judge(&arrived, &arrived[..1], Some(1)), Err(Breach::NewestLost { newest: w(2, 2), found: Some(1) }));
    check!("nothing accepted", judge(&arrived, &[], None), Err(Breach::NewestLost { newest: w(2, 2), found: None }));
}

#[test]
fn newest_means_highest_token_not_last_to_arrive() {
    let arrived = [w(2, 5), w(1, 9)];
    check!("a rejected late arrival", judge(&arrived, &arrived[..1], Some(5)), Ok(()));
}

#[test]
fn within_one_token_the_larger_value_is_newer() {
    // two writes of one holder overtook each other
    let writes = [w(1, 8), w(1, 7)];
    check!("8 was sent later", judge(&writes, &writes, Some(7)), Err(Breach::NewestLost { newest: w(1, 8), found: Some(7) }));
    check!("a disk that kept 8", judge(&writes, &writes, Some(8)), Ok(()));
}

#[test]
fn the_guard_remembers_the_highest_token() {
    let mut disk = Disk::new(true);
    disk.write(w(1, 1));
    disk.write(w(7, 2));
    check!("3 is below 7", disk.write(w(3, 3)), false);
    check!("still below 7", disk.write(w(4, 4)), false);
    check!("7 again", disk.write(w(7, 5)), true);
    check!("accepted values", disk.accepted().iter().map(|x| x.value).collect::<Vec<_>>(), vec![1, 2, 5]);
}

#[test]
fn random_writes_match_a_slow_judge() {
    let mut rng = anneal_prelude::Rng::new(42_101);
    let mut seen = [0u32; 3];
    for round in 0..600 {
        let guarded = rng.bool();
        let mut disk = Disk::new(guarded);
        let mut arrived: Vec<Write> = Vec::new();
        let mut accepted: Vec<Write> = Vec::new();
        let n = rng.below(7);
        for i in 0..n {
            let write = w(rng.int(1, 4) as u64, 100 + i as u64);
            let take = !guarded || accepted.iter().all(|a| a.token <= write.token);
            check!(format!("round {round}, guarded {guarded}, after {arrived:?}: {write:?}"), disk.write(write), take);
            arrived.push(write);
            if take {
                accepted.push(write);
            }
        }
        check!(format!("round {round}: arrived"), disk.arrived().to_vec(), arrived.clone());
        check!(format!("round {round}: accepted"), disk.accepted().to_vec(), accepted.clone());
        check!(format!("round {round}: value"), disk.value(), accepted.last().map(|a| a.value));

        // judge the real histories, and sometimes a broken end value
        let value = if rng.below(4) == 0 { Some(100 + rng.below(7) as u64) } else { disk.value() };
        let stale = (0..accepted.len()).find(|&j| accepted[..j].iter().any(|a| a.token > accepted[j].token));
        let mut by_age = arrived.clone();
        by_age.sort_by_key(|a| (a.token, a.value));
        let want = match (stale, by_age.last()) {
            (Some(j), _) => Err(Breach::StaleWrite { newer: accepted[j - 1], stale: accepted[j] }),
            (None, Some(&newest)) if value != Some(newest.value) => Err(Breach::NewestLost { newest, found: value }),
            _ => Ok(()),
        };
        seen[match want {
            Ok(()) => 0,
            Err(Breach::StaleWrite { .. }) => 1,
            Err(Breach::NewestLost { .. }) => 2,
        }] += 1;
        check!(format!("round {round}: judge {arrived:?} {accepted:?} {value:?}"), judge(&arrived, &accepted, value), want);
    }
    check!("every verdict came up", seen.iter().all(|&n| n > 20), true);
}

#[test]
fn a_long_history_is_judged_quickly() {
    let mut writes: Vec<Write> = (0..200_000u64).map(|i| w(1 + i / 100, i)).collect();
    check!("fine", judge(&writes, &writes, Some(199_999)), Ok(()));
    writes.push(w(1, 200_000));
    let stale = judge(&writes, &writes, Some(200_000));
    check!("the very last is stale", stale, Err(Breach::StaleWrite { newer: w(2000, 199_999), stale: w(1, 200_000) }));
}
