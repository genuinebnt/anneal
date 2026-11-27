#[derive(Clone)]
pub struct Door {
    // TODO: your fields
}

/// Somebody who is inside. They leave when this is dropped.
pub struct Visit {
    // TODO: your fields
}

impl Door {
    pub fn new() -> Self {
        todo!()
    }

    pub fn enter(&self) -> Option<Visit> {
        todo!()
    }

    pub fn inside(&self) -> usize {
        todo!()
    }

    pub fn is_closed(&self) -> bool {
        todo!()
    }

    pub fn close(&self) {
        todo!()
    }

    pub async fn shutdown(&self) -> u64 {
        todo!()
    }
}
