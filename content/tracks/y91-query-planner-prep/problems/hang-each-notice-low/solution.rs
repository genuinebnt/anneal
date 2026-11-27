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

/// The set of rooms in a place, as bits, and the same for its two parts. Worked out once.
struct Covered {
    rooms: u64,
    parts: Option<Box<(Covered, Covered)>>,
}

fn cover(place: &Place) -> Covered {
    match place {
        Place::Room(n) => Covered {
            rooms: 1 << n,
            parts: None,
        },
        Place::Wing(a, b) => {
            let (a, b) = (cover(a), cover(b));
            Covered {
                rooms: a.rooms | b.rooms,
                parts: Some(Box::new((a, b))),
            }
        }
    }
}

/// Go down while one side still has every wanted room. A notice for no room stays at the entrance.
fn way(building: &Covered, wanted: u64) -> Vec<Turn> {
    let mut turns = Vec::new();
    let mut here = building;
    if wanted == 0 {
        return turns;
    }
    while let Some(parts) = &here.parts {
        let (left, right) = &**parts;
        let (turn, next) = if wanted & !left.rooms == 0 {
            (Turn::Left, left)
        } else if wanted & !right.rooms == 0 {
            (Turn::Right, right)
        } else {
            break;
        };
        turns.push(turn);
        here = next;
    }
    turns
}

/// For each notice (the rooms it is meant for), the turns from the entrance to where it should hang.
/// `None` for a notice that names a room the building does not have.
pub fn hang(building: &Place, notices: &[Vec<u8>]) -> Vec<Option<Vec<Turn>>> {
    let building = cover(building);
    notices
        .iter()
        .map(|rooms| {
            let wanted = rooms.iter().try_fold(0u64, |set, &room| {
                let bit = 1u64.checked_shl(u32::from(room))?;
                (building.rooms & bit != 0).then_some(set | bit)
            })?;
            Some(way(&building, wanted))
        })
        .collect()
}
