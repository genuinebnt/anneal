/// Parcels are numbered `0..parcels`. A load is a set of them, as bits.
/// `fits(load)` says whether one van may carry exactly these parcels. Each call is counted.
///
/// Returns as few loads as possible that carry every parcel once, or `None` if that cannot be done.
pub fn fewest_vans(parcels: usize, fits: &mut dyn FnMut(u32) -> bool) -> Option<Vec<u32>> {
    let sets = 1usize << parcels;
    // for each set of parcels: the number of vans it needs, and one load of such an answer
    let mut best: Vec<Option<(u32, u32)>> = vec![None; sets];
    best[0] = Some((0, 0));
    // every part of a set is a smaller number, so it is already answered
    for set in 1..sets as u32 {
        // some van carries the lowest parcel: try only the loads that include it
        let lowest = 1 << set.trailing_zeros();
        let others = set ^ lowest;
        let mut extra = others;
        loop {
            let load = lowest | extra;
            if let Some((vans, _)) = best[(set ^ load) as usize] {
                let better = best[set as usize].is_none_or(|b| vans + 1 < b.0);
                if better && fits(load) {
                    best[set as usize] = Some((vans + 1, load));
                }
            }
            if extra == 0 {
                break;
            }
            // the next smaller subset of `others`
            extra = (extra - 1) & others;
        }
    }
    let mut left = (sets - 1) as u32;
    let mut loads = Vec::new();
    while left != 0 {
        let (_, load) = best[left as usize]?;
        loads.push(load);
        left ^= load;
    }
    Some(loads)
}
