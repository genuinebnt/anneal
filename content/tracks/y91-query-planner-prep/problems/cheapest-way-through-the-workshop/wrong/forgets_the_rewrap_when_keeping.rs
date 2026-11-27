use std::collections::BTreeMap;

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

/// The cheapest way to reach one wrapping after some station: its price, the wrapping before that station, and
/// the machine used there.
#[derive(Clone, Copy)]
struct Best {
    price: u64,
    before: u8,
    machine: usize,
}

/// The lowest total price to take a parcel through every station in order, and the machine used at each.
/// `None` if that cannot be done.
pub fn cheapest_route(workshop: &dyn Workshop, rewrap: u64) -> Option<(u64, Vec<usize>)> {
    let start = Best {
        price: 0,
        before: 0,
        machine: 0,
    };
    // one layer per station: the best way to each wrapping the parcel can have after it
    let mut layers = vec![BTreeMap::from([(0u8, start)])];
    for station in 0..workshop.stations() {
        let mut next: BTreeMap<u8, Best> = BTreeMap::new();
        let last = layers.last().expect("never empty");
        for number in 0..workshop.machines(station) {
            let machine = workshop.machine(station, number);
            for (&wrapping, so_far) in last {
                let inside = machine.needs.unwrap_or(wrapping);
                let change = if inside == wrapping { 0 } else { rewrap };
                let found = Best {
                    price: so_far.price + change + machine.price,
                    before: wrapping,
                    machine: number,
                };
                let out = machine.leaves.unwrap_or(wrapping);
                if next.get(&out).is_none_or(|b| found.price < b.price) {
                    next.insert(out, found);
                }
            }
        }
        if next.is_empty() {
            return None;
        }
        layers.push(next);
    }
    let (&end, best) = layers.last()?.iter().min_by_key(|(_, b)| b.price)?;
    let total = best.price;
    let mut wrapping = end;
    let mut route = Vec::new();
    for layer in layers.iter().skip(1).rev() {
        let step = layer[&wrapping];
        route.push(step.machine);
        wrapping = step.before;
    }
    route.reverse();
    Some((total, route))
}
