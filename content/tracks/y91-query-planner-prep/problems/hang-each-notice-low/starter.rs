/// A building: a single room, or two parts side by side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Room(u8),
    Wing(Box<Place>, Box<Place>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    Left,
    Right,
}

/// For each notice (the rooms it is meant for), the turns from the entrance to where it should hang.
/// `None` for a notice that names a room the building does not have.
pub fn hang(building: &Place, notices: &[Vec<u8>]) -> Vec<Option<Vec<Turn>>> {
    todo!()
}
