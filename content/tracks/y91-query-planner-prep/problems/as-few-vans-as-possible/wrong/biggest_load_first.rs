pub fn fewest_vans(parcels: usize, fits: &mut dyn FnMut(u32) -> bool) -> Option<Vec<u32>> {
    let mut left: u32 = (1 << parcels) - 1;
    let mut loads = Vec::new();
    while left != 0 {
        let mut chosen = 0u32;
        let mut sub = left;
        while sub != 0 {
            if sub.count_ones() > chosen.count_ones() && fits(sub) {
                chosen = sub;
            }
            sub = (sub - 1) & left;
        }
        if chosen == 0 {
            return None;
        }
        loads.push(chosen);
        left ^= chosen;
    }
    Some(loads)
}
