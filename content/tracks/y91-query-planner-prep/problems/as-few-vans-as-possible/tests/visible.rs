use solution::*;

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

#[test]
fn by_weight() {
    let weights = [4, 4, 4, 6, 6, 6];
    let rule = move |load| weight(load, &weights) <= 10;
    let loads = ask(6, &rule);
    check!("three vans of 10", loads.as_ref().map(|l| l.len()), Some(3));
    check!("a proper answer", loads.is_some_and(|l| proper(6, &rule, &l)), true);
}

#[test]
fn a_parcel_that_fits_nowhere() {
    check!("parcel 1 fits in no load", ask(3, &|load| load & 0b010 == 0), None);
}

#[test]
fn no_parcels() {
    check!("nothing to carry", ask(0, &|_| true), Some(vec![]));
}

#[test]
fn everything_in_one_van() {
    check!("any load fits", ask(5, &|_| true), Some(vec![0b11111]));
}

#[test]
fn a_load_can_fit_although_a_part_of_it_does_not() {
    // parcel 0 may only travel together with parcel 1
    let rule = |load: u32| load == 0b011 || load == 0b100;
    let loads = ask(3, &rule);
    check!("two vans", loads.as_ref().map(Vec::len), Some(2));
    check!("a proper answer", loads.is_some_and(|l| proper(3, &rule, &l)), true);
}
