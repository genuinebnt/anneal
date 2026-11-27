fn search(left: u32, fits: &mut dyn FnMut(u32) -> bool) -> Option<Vec<u32>> {
    if left == 0 {
        return Some(Vec::new());
    }
    let lowest = left & left.wrapping_neg();
    let others = left ^ lowest;
    let mut best: Option<Vec<u32>> = None;
    let mut extra = others;
    loop {
        let load = lowest | extra;
        if fits(load) {
            if let Some(mut rest) = search(left ^ load, fits) {
                rest.push(load);
                if best.as_ref().is_none_or(|b| rest.len() < b.len()) {
                    best = Some(rest);
                }
            }
        }
        if extra == 0 {
            break;
        }
        extra = (extra - 1) & others;
    }
    best
}

pub fn fewest_vans(parcels: usize, fits: &mut dyn FnMut(u32) -> bool) -> Option<Vec<u32>> {
    search((1 << parcels) - 1, fits)
}
