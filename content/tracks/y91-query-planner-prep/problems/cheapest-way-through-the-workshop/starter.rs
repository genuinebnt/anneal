#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    /// The wrapping the parcel must have to go in; `None` takes any.
    pub needs: Option<u8>,
    /// The wrapping the parcel has when it comes out; `None` leaves it as it went in.
    pub leaves: Option<u8>,
    pub price: u64,
}

/// A row of stations, each with its own machines. Each call of `machine` is counted.
pub trait Workshop {
    fn stations(&self) -> usize;
    fn machines(&self, station: usize) -> usize;
    fn machine(&self, station: usize, number: usize) -> Machine;
}

/// The lowest total price to take a parcel through every station in order, and the machine used at each.
/// `None` if that cannot be done.
pub fn cheapest_route(workshop: &dyn Workshop, rewrap: u64) -> Option<(u64, Vec<usize>)> {
    todo!()
}
