use solution::*;
use std::cell::Cell;

/// A workshop that fails the test if its machines are looked at too often.
#[allow(dead_code)]
struct Counted {
    stations: Vec<Vec<Machine>>,
    looks: Cell<usize>,
    budget: usize,
}

#[allow(dead_code)]
impl Counted {
    fn new(stations: Vec<Vec<Machine>>) -> Counted {
        // 300 looks at every machine is plenty: there are only 256 wrappings
        let budget = 300 * (stations.iter().map(Vec::len).sum::<usize>() + 1);
        Counted { stations, looks: Cell::new(0), budget }
    }
}

impl Workshop for Counted {
    fn stations(&self) -> usize {
        self.stations.len()
    }
    fn machines(&self, s: usize) -> usize {
        self.stations[s].len()
    }
    fn machine(&self, s: usize, n: usize) -> Machine {
        self.looks.set(self.looks.get() + 1);
        assert!(self.looks.get() <= self.budget, "machines were looked at more than {} times", self.budget);
        self.stations[s][n]
    }
}

#[allow(dead_code)]
fn m(needs: Option<u8>, leaves: Option<u8>, price: u64) -> Machine {
    Machine { needs, leaves, price }
}

/// The price of one route, worked out step by step.
#[allow(dead_code)]
fn price_of(stations: &[Vec<Machine>], rewrap: u64, route: &[usize]) -> Option<u64> {
    if route.len() != stations.len() {
        return None;
    }
    let (mut wrapping, mut total) = (0u8, 0u64);
    for (station, &n) in stations.iter().zip(route) {
        let machine = station.get(n)?;
        if let Some(w) = machine.needs {
            if w != wrapping {
                total += rewrap;
                wrapping = w;
            }
        }
        total += machine.price;
        wrapping = machine.leaves.unwrap_or(wrapping);
    }
    Some(total)
}

#[test]
fn the_cheapest_machine_first_is_not_always_best() {
    let stations = vec![
        vec![m(None, Some(1), 1), m(None, Some(2), 3)],
        vec![m(Some(2), None, 1)],
    ];
    let got = cheapest_route(&Counted::new(stations), 10);
    check!("start with wrapping 0, rewrap costs 10", got, Some((4, vec![1, 0])));
}

#[test]
fn rewrapping_is_paid_when_needed() {
    let stations = vec![vec![m(Some(5), None, 2)], vec![m(Some(5), Some(0), 2)], vec![m(Some(0), None, 2)]];
    check!("one rewrap", cheapest_route(&Counted::new(stations), 100), Some((106, vec![0, 0, 0])));
}

#[test]
fn no_stations_and_no_machines() {
    check!("no stations", cheapest_route(&Counted::new(vec![]), 1), Some((0, vec![])));
    check!("a station without machines", cheapest_route(&Counted::new(vec![vec![m(None, None, 1)], vec![]]), 1), None);
}

#[test]
fn a_dearer_machine_can_save_a_rewrap_much_later() {
    let stations = vec![
        vec![m(None, Some(1), 1), m(None, Some(2), 5)],
        vec![m(None, None, 1)],
        vec![m(None, None, 1)],
        vec![m(Some(2), None, 1)],
    ];
    check!("rewrap costs 20", cheapest_route(&Counted::new(stations), 20), Some((8, vec![1, 0, 0, 0])));
}

#[test]
fn one_station() {
    let stations = vec![vec![m(Some(0), None, 9), m(Some(1), None, 3)]];
    check!("rewrap 5 and pay 3, or pay 9", cheapest_route(&Counted::new(stations), 5), Some((8, vec![1])));
}
