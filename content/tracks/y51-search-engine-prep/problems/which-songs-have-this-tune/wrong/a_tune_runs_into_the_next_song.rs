use std::collections::HashMap;

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

/// The first index at or after `from` whose entry is not below `wanted`.
fn first_not_below(list: &dyn PlaceList, from: usize, wanted: (u32, usize)) -> usize {
    let (mut low, mut high) = (from, list.len());
    while low < high {
        let middle = low + (high - low) / 2;
        if list.get(middle) < wanted {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

/// `notes[i]` holds the places of note i of a tune. The `(song, start)` where the tune stands, rising.
pub fn starts(notes: &[&dyn PlaceList]) -> Vec<(u32, usize)> {
    let Some(rarest) = (0..notes.len()).min_by_key(|&i| notes[i].len()) else {
        return Vec::new();
    };
    let mut from = vec![0; notes.len()];
    let mut found = Vec::new();
    'places: for index in 0..notes[rarest].len() {
        let (song, place) = notes[rarest].get(index);
        let Some(start) = place.checked_sub(rarest) else {
            continue;
        };
        for (i, list) in notes.iter().enumerate() {
            if i == rarest {
                continue;
            }
            let Some(at) = start.checked_add(i) else {
                continue 'places;
            };
            let wanted = (song, at);
            from[i] = first_not_below(*list, from[i], wanted);
            if from[i] == list.len() || list.get(from[i]) != wanted {
                continue 'places;
            }
        }
        found.push((song, start));
    }
    found
}

pub struct Songbook {
    /// every song written one after another
    notes: Vec<u32>,
    starts: Vec<usize>,
    places: HashMap<u32, Vec<usize>>,
}

impl Songbook {
    pub fn new() -> Songbook {
        Songbook {
            notes: Vec::new(),
            starts: Vec::new(),
            places: HashMap::new(),
        }
    }

    pub fn add(&mut self, notes: &[u32]) -> u32 {
        self.starts.push(self.notes.len());
        for &note in notes {
            self.places.entry(note).or_default().push(self.notes.len());
            self.notes.push(note);
        }
        self.starts.len() as u32 - 1
    }

    pub fn find(&self, tune: &[u32]) -> Vec<(u32, usize)> {
        let rarest = tune
            .iter()
            .enumerate()
            .min_by_key(|(_, n)| self.places.get(n).map_or(0, Vec::len));
        let Some((offset, note)) = rarest else {
            return Vec::new();
        };
        self.places
            .get(note)
            .into_iter()
            .flatten()
            .filter_map(|&at| at.checked_sub(offset))
            .filter(|&at| self.notes.get(at..at + tune.len()) == Some(tune))
            .map(|at| {
                let song = self.starts.partition_point(|&s| s <= at) - 1;
                (song as u32, at - self.starts[song])
            })
            .collect()
    }
}
