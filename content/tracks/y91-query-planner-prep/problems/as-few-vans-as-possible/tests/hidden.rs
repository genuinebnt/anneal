use solution::*;
use anneal_prelude::Rng;

/// Run a search with a `fits` that fails the test if it is asked more than 3 to the power of `parcels` times.
#[allow(dead_code)]
fn ask(parcels: usize, rule: &dyn Fn(u32) -> bool) -> Option<Vec<u32>> {
    let budget = 3usize.pow(parcels as u32) + 1;
    let mut calls = 0;
    let mut fits = |load: u32| {
        calls += 1;
        assert!(calls <= budget, "fits was called more than {budget} times for {parcels} parcels");
        assert!(load != 0 && load < 1 << parcels, "fits was asked about {load:#b}, which is not a load of these parcels");
        rule(load)
    };
    fewest_vans(parcels, &mut fits)
}

/// Whether the loads carry every parcel exactly once and each fits.
#[allow(dead_code)]
fn proper(parcels: usize, rule: &dyn Fn(u32) -> bool, loads: &[u32]) -> bool {
    let mut seen = 0;
    for &load in loads {
        if load == 0 || load & seen != 0 || !rule(load) {
            return false;
        }
        seen |= load;
    }
    seen == (1u32 << parcels) - 1
}

#[allow(dead_code)]
fn weight(load: u32, weights: &[u32]) -> u32 {
    weights.iter().enumerate().filter(|(i, _)| load >> i & 1 == 1).map(|(_, w)| w).sum()
}

/// Fewest vans, found another way: grow the set of carried parcels one van at a time.
#[allow(dead_code)]
fn by_waves(parcels: usize, table: &[bool]) -> Option<usize> {
    let full = (1usize << parcels) - 1;
    let mut seen = vec![false; full + 1];
    seen[0] = true;
    let mut wave = vec![0usize];
    let mut vans = 0;
    while !wave.is_empty() {
        if seen[full] {
            return Some(vans);
        }
        vans += 1;
        let mut next = Vec::new();
        for carried in wave {
            for load in 1..=full {
                if load & carried == 0 && table[load] && !seen[carried | load] {
                    seen[carried | load] = true;
                    next.push(carried | load);
                }
            }
        }
        wave = next;
    }
    None
}

#[allow(dead_code)]
fn judge(label: String, parcels: usize, table: Vec<bool>) {
    let want = by_waves(parcels, &table);
    let rule = |load: u32| table[load as usize];
    let got = ask(parcels, &rule);
    check!(format!("{label}: vans"), got.as_ref().map(Vec::len), want);
    if let Some(loads) = got {
        check!(format!("{label}: {loads:?} is a proper answer"), proper(parcels, &rule, &loads), true);
    }
}

#[test]
fn any_rule_at_all() {
    // a van's rule need not make sense: a load may fit although part of it does not
    let mut rng = Rng::new(9181);
    for case in 0..1500 {
        let parcels = 1 + rng.below(6);
        let often = 1 + rng.below(5);
        let table: Vec<bool> = (0..1 << parcels).map(|_| rng.below(6) < often).collect();
        let listed: Vec<usize> = (1..table.len()).filter(|&i| table[i]).collect();
        judge(format!("case {case}: {parcels} parcels, loads that fit: {listed:?}"), parcels, table);
    }
}

#[test]
fn weights_and_parcels_that_cannot_travel_together() {
    let mut rng = Rng::new(9182);
    for case in 0..300 {
        let parcels = 1 + rng.below(8);
        let weights: Vec<u32> = (0..parcels).map(|_| rng.int(1, 10) as u32).collect();
        let apart: Vec<u32> = (0..rng.below(5)).map(|_| (1 << rng.below(parcels)) | (1 << rng.below(parcels))).collect();
        let table = (0..1u32 << parcels)
            .map(|l| weight(l, &weights) <= 12 && apart.iter().all(|&pair| pair.count_ones() < 2 || l & pair != pair))
            .collect();
        judge(format!("case {case}: weights {weights:?}, apart {apart:?}"), parcels, table);
    }
}

#[test]
fn loads_that_only_fit_in_pairs() {
    let table: Vec<bool> = (0..1u32 << 10).map(|l| l.count_ones() == 2).collect();
    judge("ten parcels, only pairs".to_string(), 10, table);
    let odd: Vec<bool> = (0..1u32 << 9).map(|l| l.count_ones() == 2).collect();
    judge("nine parcels, only pairs".to_string(), 9, odd);
}

#[test]
fn twelve_parcels_that_each_need_a_van() {
    judge("twelve vans".to_string(), 12, (0..1u32 << 12).map(|l| l.count_ones() == 1).collect());
}

#[test]
fn twelve_parcels_by_weight() {
    let weights = [9, 8, 7, 7, 6, 5, 5, 4, 3, 3, 2, 1];
    judge("sixty units into vans of fifteen".to_string(), 12, (0..1u32 << 12).map(|l| weight(l, &weights) <= 15).collect());
}

#[test]
fn twelve_parcels_that_cannot_all_travel() {
    judge("parcel 11 never fits".to_string(), 12, (0..1u32 << 12).map(|l| l >> 11 & 1 == 0 && l.count_ones() <= 3).collect());
}

#[test]
fn one_parcel() {
    judge("it fits".to_string(), 1, vec![false, true]);
    judge("it does not".to_string(), 1, vec![false, false]);
}

#[test]
fn only_all_together() {
    let table: Vec<bool> = (0..1u32 << 11).map(|l| l == (1 << 11) - 1).collect();
    judge("eleven parcels in one van or none".to_string(), 11, table);
}
