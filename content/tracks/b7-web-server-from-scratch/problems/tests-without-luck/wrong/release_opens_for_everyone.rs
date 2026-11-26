use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
struct State {
    arrived: usize,
    /// Passes granted and not yet used: kept, so that a release before the arrival is not lost.
    permits: usize,
    open: bool,
}

#[derive(Clone, Default)]
pub struct Gate {
    shared: Arc<(Mutex<State>, Condvar)>,
}

impl Gate {
    pub fn new() -> Gate {
        Gate::default()
    }

    /// Changes the state and wakes everyone: arrivals and releases are waited for on the same condition variable.
    fn change(&self, f: impl FnOnce(&mut State)) {
        let (state, changed) = &*self.shared;
        f(&mut state.lock().unwrap_or_else(|e| e.into_inner()));
        changed.notify_all();
    }

    pub fn pass(&self) {
        self.change(|s| s.arrived += 1);
        let (state, changed) = &*self.shared;
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = changed.wait_while(guard, |s| !s.open && s.permits == 0).unwrap_or_else(|e| e.into_inner());
    }

    pub fn release(&self, n: usize) {
        self.change(|s| s.permits += n);
    }

    pub fn open(&self) {
        self.change(|s| s.open = true);
    }

    pub fn arrived(&self) -> usize {
        self.shared.0.lock().unwrap_or_else(|e| e.into_inner()).arrived
    }

    pub fn wait_for_arrivals(&self, n: usize, timeout: Duration) -> bool {
        let (state, changed) = &*self.shared;
        let guard = state.lock().unwrap_or_else(|e| e.into_inner());
        // one total timeout, whatever the number of wake-ups
        let (guard, _) = changed.wait_timeout_while(guard, timeout, |s| s.arrived < n).unwrap_or_else(|e| e.into_inner());
        guard.arrived >= n
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Data(Vec<u8>),
    Quiet,
    Interrupt,
    End,
}

pub struct Script {
    events: VecDeque<Event>,
    written: Vec<u8>,
    write_limit: usize,
}

impl Script {
    pub fn new(events: Vec<Event>) -> Script {
        Script { events: events.into(), written: Vec::new(), write_limit: usize::MAX }
    }

    pub fn limit_writes(&mut self, n: usize) {
        assert!(n >= 1);
        self.write_limit = n;
    }

    pub fn written(&self) -> &[u8] {
        &self.written
    }

    pub fn unread(&self) -> usize {
        self.events.iter().map(|e| if let Event::Data(d) = e { d.len() } else { 0 }).sum()
    }
}

impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            match self.events.front_mut() {
                // `End` stays at the front: the end of a stream does not go away
                None | Some(Event::End) => return Ok(0),
                Some(Event::Quiet) => {
                    self.events.pop_front();
                    return Err(ErrorKind::WouldBlock.into());
                }
                Some(Event::Interrupt) => {
                    self.events.pop_front();
                    return Err(ErrorKind::Interrupted.into());
                }
                Some(Event::Data(data)) => {
                    let n = data.len().min(buf.len());
                    buf[..n].copy_from_slice(&data[..n]);
                    data.drain(..n);
                    if data.is_empty() {
                        self.events.pop_front();
                    }
                    // an empty event must not look like the end of the stream
                    if n > 0 {
                        return Ok(n);
                    }
                }
            }
        }
    }
}

impl Write for Script {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = buf.len().min(self.write_limit);
        self.written.extend_from_slice(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
