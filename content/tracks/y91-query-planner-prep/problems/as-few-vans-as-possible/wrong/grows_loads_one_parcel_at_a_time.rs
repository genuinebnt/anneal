pub fn fewest_vans(parcels: usize, fits: &mut dyn FnMut(u32) -> bool) -> Option<Vec<u32>> {
    let mut loads: Vec<u32> = Vec::new();
    for p in 0..parcels {
        let bit = 1u32 << p;
        match loads.iter().position(|&l| fits(l | bit)) {
            Some(i) => loads[i] |= bit,
            None if fits(bit) => loads.push(bit),
            None => return None,
        }
    }
    Some(loads)
}
