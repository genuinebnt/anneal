use std::collections::VecDeque;

/// The largest payload a packet event carries.
pub const MTU: usize = 1500;

/// What the network thread hands the connection thread: millions a second, through a queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Tick(u64),
    Ack {
        conn: u32,
        seq: u32,
    },
    Close(u32),
    Packet {
        conn: u32,
        len: u16,
        data: [u8; MTU],
    },
}

impl Event {
    /// A packet event for `conn` carrying `payload`. Panics if `payload` is longer than `MTU`.
    pub fn packet(conn: u32, payload: &[u8]) -> Event {
        assert!(
            payload.len() <= MTU,
            "payload of {} bytes is over the MTU",
            payload.len()
        );
        let mut data = [0; MTU];
        data[..payload.len()].copy_from_slice(payload);
        Event::Packet {
            conn,
            len: payload.len() as u16,
            data,
        }
    }

    /// The connection an event is about, if any.
    pub fn conn(&self) -> Option<u32> {
        match self {
            Event::Tick(_) => None,
            Event::Ack { conn, .. } | Event::Packet { conn, .. } => Some(*conn),
            Event::Close(conn) => Some(*conn),
        }
    }

    pub fn payload(&self) -> Option<&[u8]> {
        match self {
            Event::Packet { len, data, .. } => Some(&data[..*len as usize]),
            _ => None,
        }
    }
}

/// What the connection thread saw in one batch.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub last_tick: u64,
    pub acks: usize,
    pub packets: usize,
    pub bytes: usize,
    pub closed: Vec<u32>,
}

/// Empties `queue`, oldest first.
pub fn drain(queue: &mut VecDeque<Event>) -> Summary {
    let mut s = Summary::default();
    while let Some(ev) = queue.pop_front() {
        match &ev {
            Event::Tick(t) => s.last_tick = *t,
            Event::Ack { .. } => s.acks += 1,
            Event::Close(conn) => s.closed.push(*conn),
            _ => {
                s.packets += 1;
                s.bytes += ev.payload().map_or(0, <[u8]>::len);
            }
        }
    }
    s
}
