use solution::*;

use std::collections::HashMap;

/// A wire that answers by a script: for each order, how many sends are lost before one is acked.
#[allow(dead_code)]
#[derive(Default)]
struct Net {
    lose: HashMap<u64, u32>,
    sent: Vec<Notice>,
}

impl Wire for Net {
    fn send(&mut self, notice: &Notice) -> Delivery {
        self.sent.push(*notice);
        match self.lose.get_mut(&notice.order) {
            Some(n) if *n > 0 => {
                *n -= 1;
                Delivery::Unknown
            }
            _ => Delivery::Acked,
        }
    }
}

#[allow(dead_code)]
fn log_of(items: &[(u64, Change)]) -> Vec<Entry> {
    items.iter().map(|&(order, change)| Entry { order, change }).collect()
}

#[allow(dead_code)]
fn ids(sent: &[Notice]) -> Vec<u64> {
    sent.iter().map(|n| n.id).collect()
}

#[test]
fn before_the_first_run() {
    let relay = Relay::resume(7);
    check!("saved", relay.saved(), 7);
    check!("waiting", relay.waiting(), 0);
}

#[test]
fn resuming_sends_nothing_before_the_position() {
    let log = log_of(&[(1, Change::Paid), (2, Change::Paid), (3, Change::Paid)]);
    let mut relay = Relay::resume(2);
    let mut net = Net::default();
    relay.run(&log, &mut net);
    check!("ids", ids(&net.sent), vec![2]);
}

#[test]
fn the_oldest_waiting_one_decides() {
    let log = log_of(&[(1, Change::Paid), (2, Change::Paid), (3, Change::Paid)]);
    let mut relay = Relay::resume(0);
    let mut net = Net { lose: HashMap::from([(2, 2), (3, 1)]), ..Net::default() };
    relay.run(&log, &mut net);
    check!("saved", relay.saved(), 1);
    relay.run(&log, &mut net);
    check!("saved", relay.saved(), 1);
    relay.run(&log, &mut net);
    check!("saved", relay.saved(), 3);
    check!("oldest first each time", ids(&net.sent), vec![0, 1, 2, 1, 2, 1]);
}

#[test]
fn the_log_grows_between_runs() {
    let mut log = log_of(&[(1, Change::Paid)]);
    let mut relay = Relay::resume(0);
    let mut net = Net { lose: HashMap::from([(1, 1)]), ..Net::default() };
    relay.run(&log, &mut net);
    log.push(Entry { order: 2, change: Change::Cancelled });
    relay.run(&log, &mut net);
    check!("old waiting one first", ids(&net.sent), vec![0, 0, 1]);
    relay.run(&log, &mut net);
    check!("nothing more", net.sent.len(), 3);
}

#[test]
fn an_empty_log() {
    let mut relay = Relay::resume(0);
    let mut net = Net::default();
    relay.run(&[], &mut net);
    check!("saved", relay.saved(), 0);
}

/// A wire into a real inbox that loses messages both ways.
#[allow(dead_code)]
struct Flaky<'a> {
    rng: &'a mut anneal_prelude::Rng,
    inbox: &'a mut Inbox,
    applied: &'a mut Vec<Notice>,
    reliable: bool,
}

impl Wire for Flaky<'_> {
    fn send(&mut self, notice: &Notice) -> Delivery {
        let fate = if self.reliable { 0 } else { self.rng.below(4) };
        if fate == 1 {
            return Delivery::Unknown; // lost on the way there
        }
        if self.inbox.accept(notice) {
            self.applied.push(*notice);
        }
        if fate == 2 {
            Delivery::Unknown // lost on the way back
        } else {
            Delivery::Acked
        }
    }
}

#[test]
fn crashes_and_losses_still_apply_each_notice_once() {
    let mut rng = anneal_prelude::Rng::new(7110);
    for case in 0..400 {
        let all: Vec<Entry> = (0..rng.below(20))
            .map(|_| Entry { order: rng.below(4) as u64, change: *rng.pick(&[Change::Placed, Change::Viewed, Change::Paid, Change::Cancelled]) })
            .collect();
        let mut shown = 0;
        let mut relay = Relay::resume(0);
        let mut stored = vec![0];
        let mut inbox = Inbox::new();
        let mut applied = Vec::new();
        for _ in 0..25 {
            shown = (shown + rng.below(4)).min(all.len());
            match rng.below(4) {
                0 => {
                    let from = *rng.pick(&stored);
                    relay = Relay::resume(from);
                }
                _ => {
                    let mut wire = Flaky { rng: &mut rng, inbox: &mut inbox, applied: &mut applied, reliable: false };
                    relay.run(&all[..shown], &mut wire);
                    stored.push(relay.saved());
                }
            }
        }
        // a good day at last: after one more crash, everything gets through
        let mut relay = Relay::resume(*rng.pick(&stored));
        let mut wire = Flaky { rng: &mut rng, inbox: &mut inbox, applied: &mut applied, reliable: true };
        relay.run(&all, &mut wire);
        let input = format!("case {case}: {all:?}");
        check!(input.clone(), (relay.waiting(), relay.saved()), (0, all.len() as u64));
        applied.sort_by_key(|n| n.id);
        let want: Vec<Notice> = all
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e.change, Change::Paid | Change::Cancelled))
            .map(|(i, e)| Notice { id: i as u64, order: e.order, change: e.change })
            .collect();
        check!(input, applied, want);
    }
}

#[test]
fn a_long_log_that_grows_one_entry_at_a_time() {
    let log: Vec<Entry> = (0..200_000).map(|i| Entry { order: i, change: if i % 2 == 0 { Change::Paid } else { Change::Viewed } }).collect();
    let mut relay = Relay::resume(0);
    let mut net = Net::default();
    for end in 1..=log.len() {
        relay.run(&log[..end], &mut net);
    }
    check!("sent", net.sent.len(), 100_000);
    check!("saved", relay.saved(), 200_000);
}

#[test]
fn each_notice_is_judged_by_its_id() {
    // the same order is paid, cancelled and paid again: three different notices
    let log = log_of(&[(1, Change::Paid), (1, Change::Cancelled), (1, Change::Paid)]);
    let mut relay = Relay::resume(0);
    let mut net = Net::default();
    relay.run(&log, &mut net);
    let mut inbox = Inbox::new();
    let first: Vec<bool> = net.sent.iter().map(|n| inbox.accept(n)).collect();
    check!("all new", first, vec![true, true, true]);
    let again: Vec<bool> = net.sent.iter().map(|n| inbox.accept(n)).collect();
    check!("all old", again, vec![false, false, false]);
}
