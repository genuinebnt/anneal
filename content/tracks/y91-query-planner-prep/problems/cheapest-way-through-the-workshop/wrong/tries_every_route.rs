#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Machine {
    pub needs: Option<u8>,
    pub leaves: Option<u8>,
    pub price: u64,
}

pub trait Workshop {
    fn stations(&self) -> usize;
    fn machines(&self, station: usize) -> usize;
    fn machine(&self, station: usize, number: usize) -> Machine;
}

fn from(
    workshop: &dyn Workshop,
    rewrap: u64,
    station: usize,
    wrapping: u8,
) -> Option<(u64, Vec<usize>)> {
    if station == workshop.stations() {
        return Some((0, Vec::new()));
    }
    let mut best: Option<(u64, Vec<usize>)> = None;
    for number in 0..workshop.machines(station) {
        let machine = workshop.machine(station, number);
        let inside = machine.needs.unwrap_or(wrapping);
        let change = if inside == wrapping { 0 } else { rewrap };
        let out = machine.leaves.unwrap_or(inside);
        if let Some((price, mut route)) = from(workshop, rewrap, station + 1, out) {
            let price = price + change + machine.price;
            if best.as_ref().is_none_or(|b| price < b.0) {
                route.insert(0, number);
                best = Some((price, route));
            }
        }
    }
    best
}

pub fn cheapest_route(workshop: &dyn Workshop, rewrap: u64) -> Option<(u64, Vec<usize>)> {
    from(workshop, rewrap, 0, 0)
}
