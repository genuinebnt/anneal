use std::io::{self, Read, Write};
use std::time::Duration;

#[derive(Clone, Default)]
pub struct Gate {
    // TODO
}

impl Gate {
    pub fn new() -> Gate {
        todo!()
    }

    pub fn pass(&self) {
        todo!()
    }

    pub fn release(&self, n: usize) {
        todo!()
    }

    pub fn open(&self) {
        todo!()
    }

    pub fn arrived(&self) -> usize {
        todo!()
    }

    pub fn wait_for_arrivals(&self, n: usize, timeout: Duration) -> bool {
        todo!()
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
    // TODO
}

impl Script {
    pub fn new(events: Vec<Event>) -> Script {
        todo!()
    }

    pub fn limit_writes(&mut self, n: usize) {
        todo!()
    }

    pub fn written(&self) -> &[u8] {
        todo!()
    }

    pub fn unread(&self) -> usize {
        todo!()
    }
}

impl Read for Script {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        todo!()
    }
}

impl Write for Script {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        todo!()
    }

    fn flush(&mut self) -> io::Result<()> {
        todo!()
    }
}
