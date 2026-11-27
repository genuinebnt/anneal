use solution::*;
use anneal_prelude::Rng;
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

#[allow(dead_code)]
fn every_route(stations: &[Vec<Machine>], rewrap: u64, route: &mut Vec<usize>, best: &mut Option<u64>) {
    if route.len() == stations.len() {
        let p = price_of(stations, rewrap, route).unwrap();
        *best = Some(best.map_or(p, |b| b.min(p)));
        return;
    }
    for n in 0..stations[route.len()].len() {
        route.push(n);
        every_route(stations, rewrap, route, best);
        route.pop();
    }
}

#[allow(dead_code)]
fn random_machine(rng: &mut Rng, wrappings: i64) -> Machine {
    let w = |rng: &mut Rng| (rng.below(3) > 0).then(|| rng.int(0, wrappings - 1) as u8);
    let needs = w(rng);
    let leaves = w(rng);
    m(needs, leaves, rng.int(0, 20) as u64)
}

#[test]
fn small_workshops_against_every_route() {
    let mut rng = Rng::new(9171);
    for case in 0..1500 {
        let wrappings = rng.int(1, 4);
        let stations: Vec<Vec<Machine>> = (0..rng.below(6))
            .map(|_| {
                let n = if rng.below(15) == 0 { 0 } else { 1 + rng.below(4) };
                (0..n).map(|_| random_machine(&mut rng, wrappings)).collect()
            })
            .collect();
        let rewrap = [0, 1, 7, 100][rng.below(4)];
        let input = format!("case {case}: rewrap {rewrap}, stations {stations:?}");
        let mut best = None;
        if stations.iter().all(|s| !s.is_empty()) {
            every_route(&stations, rewrap, &mut Vec::new(), &mut best);
        }
        let got = cheapest_route(&Counted::new(stations.clone()), rewrap);
        check!(format!("{input}: total"), got.as_ref().map(|g| g.0), best);
        if let Some((total, route)) = got {
            check!(format!("{input}: the route {route:?} really costs {total}"), price_of(&stations, rewrap, &route), Some(total));
        }
    }
}

#[test]
fn wrapping_255_and_free_rewrapping() {
    let stations = vec![vec![m(Some(255), Some(254), 1)], vec![m(Some(254), None, 1)], vec![m(Some(255), None, 1)]];
    check!("free rewraps", cheapest_route(&Counted::new(stations), 0).map(|g| g.0), Some(3));
}

#[test]
fn large_prices() {
    let stations = vec![vec![m(Some(1), None, 1_000_000_000_000)]; 1000];
    check!("1000 stations of 10^12", cheapest_route(&Counted::new(stations), 1_000_000_000_000).map(|g| g.0), Some(1_001_000_000_000_000));
}

#[test]
fn a_long_workshop() {
    let mut rng = Rng::new(9172);
    let stations: Vec<Vec<Machine>> = (0..1000).map(|_| (0..4).map(|_| random_machine(&mut rng, 6)).collect()).collect();
    let (total, route) = cheapest_route(&Counted::new(stations.clone()), 15).expect("possible");
    check!("the route costs the total", price_of(&stations, 15, &route), Some(total));
}

#[test]
fn many_wrappings() {
    let stations: Vec<Vec<Machine>> = (0..40).map(|s| (0..200).map(|w| m(None, Some(w as u8), 1 + u64::from((w + s) % 7 == 0))).collect()).collect();
    check!("200 wrappings, 40 stations", cheapest_route(&Counted::new(stations), 50).map(|g| g.0), Some(40));
}

#[test]
fn a_machine_that_keeps_the_wrapping_keeps_the_new_one() {
    let stations = vec![vec![m(Some(3), None, 1)], vec![m(Some(3), None, 1)]];
    check!("only one rewrap", cheapest_route(&Counted::new(stations), 10).map(|g| g.0), Some(12));
}

#[test]
fn a_missing_station_in_the_middle() {
    let stations = vec![vec![m(None, None, 1)], vec![], vec![m(None, None, 1)]];
    check!("cannot get through", cheapest_route(&Counted::new(stations), 1), None);
}

#[test]
fn rewrapping_can_cost_nothing_or_a_lot() {
    let stations: Vec<Vec<Machine>> = (0..10).map(|i| vec![m(Some(i % 2), None, 1), m(None, None, 4)]).collect();
    check!("free: always the cheap machine", cheapest_route(&Counted::new(stations.clone()), 0).map(|g| g.0), Some(10));
    check!("dear: never rewrap", cheapest_route(&Counted::new(stations), 1000).map(|g| g.0), Some(25));
}
