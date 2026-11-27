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
fn only_some_entries_are_sent() {
    let log = log_of(&[(1, Change::Placed), (1, Change::Paid), (2, Change::Viewed), (2, Change::Cancelled)]);
    let mut relay = Relay::resume(0);
    let mut net = Net::default();
    relay.run(&log, &mut net);
    check!("sent", net.sent.clone(), vec![Notice { id: 1, order: 1, change: Change::Paid }, Notice { id: 3, order: 2, change: Change::Cancelled }]);
    check!("saved", relay.saved(), 4);
}

#[test]
fn a_lost_reply_is_sent_again_and_holds_the_saved_position() {
    let log = log_of(&[(1, Change::Paid), (2, Change::Paid), (3, Change::Paid)]);
    let mut relay = Relay::resume(0);
    let mut net = Net { lose: HashMap::from([(1, 1)]), ..Net::default() };
    relay.run(&log, &mut net);
    check!("all tried", ids(&net.sent), vec![0, 1, 2]);
    check!("waiting", relay.waiting(), 1);
    check!("saved", relay.saved(), 0);
    relay.run(&log, &mut net);
    check!("only the lost one again", ids(&net.sent), vec![0, 1, 2, 0]);
    check!("saved", relay.saved(), 3);
}

#[test]
fn the_saved_position_passes_what_needs_no_notice() {
    let log = log_of(&[(1, Change::Placed), (2, Change::Viewed), (3, Change::Paid), (4, Change::Viewed)]);
    let mut relay = Relay::resume(0);
    let mut net = Net { lose: HashMap::from([(3, 1)]), ..Net::default() };
    relay.run(&log, &mut net);
    check!("held at the waiting one", relay.saved(), 2);
    relay.run(&log, &mut net);
    check!("at the end", relay.saved(), 4);
}

#[test]
fn the_inbox_acts_once() {
    let mut inbox = Inbox::new();
    let n = Notice { id: 5, order: 1, change: Change::Paid };
    check!("first", inbox.accept(&n), true);
    check!("again", inbox.accept(&n), false);
    check!("same order, other entry", inbox.accept(&Notice { id: 6, order: 1, change: Change::Cancelled }), true);
}

#[test]
fn a_restart_loses_nothing() {
    let log = log_of(&[(1, Change::Paid), (2, Change::Paid)]);
    let mut relay = Relay::resume(0);
    let mut net = Net { lose: HashMap::from([(1, 1)]), ..Net::default() };
    relay.run(&log, &mut net);
    let mut relay = Relay::resume(relay.saved());
    let mut net = Net::default();
    relay.run(&log, &mut net);
    check!("sent after the restart", ids(&net.sent), vec![0, 1]);
}
