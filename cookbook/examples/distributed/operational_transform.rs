//! RECIPE 151 (distributed): operational transformation (OT) for plain text, with a central-server client/server model.
//! LEVEL: advanced
//! Use in: understanding how collaborative text editors reconcile concurrent edits through a serialising server.   Run: cargo run --release --example operational_transform
#![allow(dead_code)]
//!
//! WHAT: two users edit the same text concurrently. Op A was made against state S, op B was ALSO made against S. To apply B
//! after A we must rewrite B so it still means the same thing: B' = "B as if A had already happened". `transform(a, b)`
//! returns (a', b') such that TP1 holds: apply(apply(S,a), b') == apply(apply(S,b), a').
//! HOW (Jupiter/ot.js style): the server serialises all ops into one history. A client keeps a queue of its own unacknowledged
//! ops and sends only the head, tagged with the server revision it was based on. The server transforms the op against every
//! history entry after that revision, appends it and broadcasts it. A client receiving someone else's op transforms it through
//! its own pending queue before applying it (the queue is rewritten in the process), so everyone applies the same linear history.
//! TIE-BREAK: two inserts at the same index need a deterministic order that both sides compute identically; here the lower
//! site id stays left. Two deletes of the same char both become Nop.
//! OT vs CRDT (crdts): OT keeps documents small (no per-char ids) but needs a central or totally-ordering service, and
//! the fully decentralised variant needs the harder property TP2 (transform results independent of the order ops are
//! transformed in), which is notoriously easy to get wrong; this simple position-based transform in fact VIOLATES TP2
//! (searched for below). CRDTs (RGA in crdts) pay per-element metadata but need no server and tolerate any delivery order.
//! COMMON USES: server-mediated collaborative editors (Google Docs is widely documented as OT-based), shared notes, chat
//! drafts, any "everyone edits one document through one backend" product.
//! Read: https://en.wikipedia.org/wiki/Operational_transformation
use std::collections::VecDeque;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 11) as usize % n
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Ins { pos: usize, ch: char, site: u32 },
    Del { pos: usize },
    Nop,
}
use Op::*;

fn apply(s: &str, op: Op) -> String {
    let mut v: Vec<char> = s.chars().collect();
    match op {
        Ins { pos, ch, .. } => v.insert(pos, ch),
        Del { pos } => {
            v.remove(pos);
        }
        Nop => {}
    }
    v.into_iter().collect()
}

/// Both ops were generated against the same state. Returns (a', b'): a rewritten to run after b, b rewritten to run after a.
fn transform(a: Op, b: Op) -> (Op, Op) {
    match (a, b) {
        (Nop, _) | (_, Nop) => (a, b),
        (
            Ins {
                pos: pa,
                ch: ca,
                site: sa,
            },
            Ins {
                pos: pb,
                ch: cb,
                site: sb,
            },
        ) => {
            // equal index: the lower site keeps its index and the other shifts right, identical on both sides
            if pa < pb || (pa == pb && (sa, ca) < (sb, cb)) {
                (
                    a,
                    Ins {
                        pos: pb + 1,
                        ch: cb,
                        site: sb,
                    },
                )
            } else {
                (
                    Ins {
                        pos: pa + 1,
                        ch: ca,
                        site: sa,
                    },
                    b,
                )
            }
        }
        (Ins { pos: pa, ch, site }, Del { pos: pb }) => {
            if pa <= pb {
                (a, Del { pos: pb + 1 })
            } else {
                (
                    Ins {
                        pos: pa - 1,
                        ch,
                        site,
                    },
                    b,
                )
            }
        }
        (Del { .. }, Ins { .. }) => {
            let (b2, a2) = transform(b, a);
            (a2, b2)
        }
        (Del { pos: pa }, Del { pos: pb }) => match pa.cmp(&pb) {
            std::cmp::Ordering::Less => (a, Del { pos: pb - 1 }),
            std::cmp::Ordering::Greater => (Del { pos: pa - 1 }, b),
            std::cmp::Ordering::Equal => (Nop, Nop), // same char deleted twice: do it once
        },
    }
}

fn random_op(rng: &mut Rng, s: &str, site: u32) -> Op {
    let len = s.chars().count();
    if len == 0 || rng.below(10) < 6 {
        Ins {
            pos: rng.below(len + 1),
            ch: (b'a' + rng.below(26) as u8) as char,
            site,
        }
    } else {
        Del {
            pos: rng.below(len),
        }
    }
}

// ---- client / server ----
#[derive(Clone, Copy, Debug)]
struct ToServer {
    client: usize,
    base_rev: usize, // number of server ops the client had applied when it (re)based this op
    op: Op,
}
#[derive(Clone, Copy, Debug)]
struct FromServer {
    author: usize,
    op: Op, // already transformed into server order
}

struct Server {
    doc: String,
    history: Vec<Op>,
}
impl Server {
    fn receive(&mut self, m: ToServer) -> FromServer {
        let mut op = m.op;
        for &h in &self.history[m.base_rev..] {
            op = transform(op, h).0; // op must also account for everything serialised since its base
        }
        self.doc = apply(&self.doc, op);
        self.history.push(op);
        FromServer {
            author: m.client,
            op,
        }
    }
}

struct Client {
    id: usize,
    doc: String,
    rev: usize,
    pending: VecDeque<Op>, // my ops not yet acknowledged; pending[0] is the one in flight
}
impl Client {
    fn local(&mut self, op: Op) -> Option<ToServer> {
        self.doc = apply(&self.doc, op);
        self.pending.push_back(op);
        (self.pending.len() == 1).then_some(ToServer {
            client: self.id,
            base_rev: self.rev,
            op,
        })
    }
    fn on_server(&mut self, m: FromServer) -> Option<ToServer> {
        self.rev += 1;
        if m.author == self.id {
            self.pending.pop_front(); // ack; its transformed form was already applied locally
            return self.pending.front().map(|&op| ToServer {
                client: self.id,
                base_rev: self.rev,
                op,
            });
        }
        let mut incoming = m.op;
        for p in self.pending.iter_mut() {
            let (inc2, p2) = transform(incoming, *p);
            incoming = inc2;
            *p = p2; // my pending op is now expressed after the incoming one
        }
        self.doc = apply(&self.doc, incoming);
        None
    }
}

fn simulate(seed: u64, n: usize, steps: usize) -> (String, Vec<String>) {
    let mut rng = Rng(seed * 2654435761 + 7);
    let mut server = Server {
        doc: String::new(),
        history: vec![],
    };
    let mut clients: Vec<Client> = (0..n)
        .map(|id| Client {
            id,
            doc: String::new(),
            rev: 0,
            pending: VecDeque::new(),
        })
        .collect();
    let mut up: Vec<VecDeque<ToServer>> = vec![VecDeque::new(); n]; // per-client FIFO channels
    let mut down: Vec<VecDeque<FromServer>> = vec![VecDeque::new(); n];
    for step in 0..steps + 4000 {
        let busy = up.iter().any(|q| !q.is_empty()) || down.iter().any(|q| !q.is_empty());
        if step >= steps && !busy {
            break;
        }
        let k = rng.below(2 * n + 1);
        if step < steps && k == 2 * n {
            let c = rng.below(n);
            let op = random_op(&mut rng, &clients[c].doc, c as u32);
            if let Some(m) = clients[c].local(op) {
                up[c].push_back(m);
            }
        } else if k < n {
            if let Some(m) = up[k].pop_front() {
                let out = server.receive(m);
                down.iter_mut().for_each(|q| q.push_back(out));
            }
        } else if k < 2 * n {
            if let Some(m) = down[k - n].pop_front() {
                if let Some(next) = clients[k - n].on_server(m) {
                    up[k - n].push_back(next);
                }
            }
        }
    }
    assert!(
        clients.iter().all(|c| c.pending.is_empty()),
        "every op must be acknowledged"
    );
    (server.doc, clients.into_iter().map(|c| c.doc).collect())
}

/// TP2 asks: transforming c over (a then b') equals c over (b then a'). Search random triples for a violation.
fn find_tp2_violation(rng: &mut Rng) -> Option<(String, [Op; 3])> {
    for _ in 0..200_000 {
        let s: String = (0..rng.below(4))
            .map(|i| (b'a' + i as u8) as char)
            .collect();
        let (a, b, c) = (
            random_op(rng, &s, 1),
            random_op(rng, &s, 2),
            random_op(rng, &s, 3),
        );
        let (a2, b2) = transform(a, b);
        let via_a = transform(transform(c, a).0, b2).0;
        let via_b = transform(transform(c, b).0, a2).0;
        if via_a != via_b {
            return Some((s, [a, b, c]));
        }
    }
    None
}

fn main() {
    // TP1 on thousands of random concurrent pairs from distinct sites.
    let mut rng = Rng(0xABCDEF12345);
    for _ in 0..50_000 {
        let s: String = (0..rng.below(8))
            .map(|i| (b'a' + i as u8) as char)
            .collect();
        let (a, b) = (random_op(&mut rng, &s, 1), random_op(&mut rng, &s, 2));
        let (a2, b2) = transform(a, b);
        assert_eq!(
            apply(&apply(&s, a), b2),
            apply(&apply(&s, b), a2),
            "TP1 failed for {a:?} {b:?} on {s:?}"
        );
    }
    println!("TP1 holds on 50000 random concurrent pairs");

    let s = "ab";
    let (a, b) = (
        Ins {
            pos: 1,
            ch: 'X',
            site: 1,
        },
        Ins {
            pos: 1,
            ch: 'Y',
            site: 2,
        },
    );
    let (a2, b2) = transform(a, b);
    println!(
        "same-position inserts: {:?} == {:?}",
        apply(&apply(s, a), b2),
        apply(&apply(s, b), a2)
    );
    assert_eq!(apply(&apply(s, a), b2), "aXYb");

    for seed in 1..=60 {
        let (server, clients) = simulate(seed, 3, 120);
        assert!(
            clients.iter().all(|c| *c == server),
            "seed {seed} diverged: server={server:?} clients={clients:?}"
        );
    }
    let (server, _) = simulate(7, 3, 120);
    println!("60 seeded runs: 3 clients + server converged (seed 7 final text = {server:?})");

    let (s, ops) = find_tp2_violation(&mut rng).expect("simple transform violates TP2");
    println!(
        "TP2 does NOT hold for this simple transform, e.g. on {s:?}: {ops:?} (why decentralised OT is hard)"
    );
}
