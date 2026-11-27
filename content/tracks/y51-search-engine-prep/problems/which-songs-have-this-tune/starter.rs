/// The places of one note, rising by song and then by place, read one at a time.
pub trait PlaceList {
    /// How many places it holds. Asking is free.
    fn len(&self) -> usize;
    /// The `(song, place)` at index `i`, which must be below `len()`. The tests count every call.
    fn get(&self, i: usize) -> (u32, usize);
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `notes[i]` holds the places of note i of a tune. The `(song, start)` where the tune stands, rising.
pub fn starts(notes: &[&dyn PlaceList]) -> Vec<(u32, usize)> {
    todo!()
}

pub struct Songbook {
    // your fields
}

impl Songbook {
    pub fn new() -> Songbook {
        todo!()
    }

    /// Stores the next song and returns its number: 0, 1, 2, ...
    pub fn add(&mut self, notes: &[u32]) -> u32 {
        todo!()
    }

    /// Every `(song, start)` where `tune` appears, sorted.
    pub fn find(&self, tune: &[u32]) -> Vec<(u32, usize)> {
        todo!()
    }
}
