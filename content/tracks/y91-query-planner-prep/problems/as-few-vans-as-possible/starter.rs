/// Parcels are numbered `0..parcels`. A load is a set of them, as bits.
/// `fits(load)` says whether one van may carry exactly these parcels. Each call is counted.
///
/// Returns as few loads as possible that carry every parcel once, or `None` if that cannot be done.
pub fn fewest_vans(parcels: usize, fits: &mut dyn FnMut(u32) -> bool) -> Option<Vec<u32>> {
    todo!()
}
