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
    /// note -> every (song, place) that has it, sorted because songs and places only grow
    places: HashMap<u32, Vec<(u32, usize)>>,
    songs: u32,
}

impl Songbook {
    pub fn new() -> Songbook {
        Songbook {
            places: HashMap::new(),
            songs: 0,
        }
    }

    /// Stores the next song and returns its number: 0, 1, 2, ...
    pub fn add(&mut self, notes: &[u32]) -> u32 {
        let song = self.songs;
        self.songs += 1;
        for (place, &note) in notes.iter().enumerate() {
            self.places.entry(note).or_default().push((song, place));
        }
        song
    }

    /// Every `(song, start)` where `tune` appears, sorted.
    pub fn find(&self, tune: &[u32]) -> Vec<(u32, usize)> {
        let mut lists = Vec::with_capacity(tune.len());
        for note in tune {
            match self.places.get(note) {
                Some(list) => lists.push(list),
                None => return Vec::new(),
            }
        }
        let rarest = lists.iter().enumerate().min_by_key(|(_, l)| l.len());
        let Some((offset, anchor)) = rarest else {
            return Vec::new();
        };
        let mut found: Vec<(u32, usize)> = anchor
            .iter()
            .filter_map(|&(song, place)| Some((song, place.checked_sub(offset)?)))
            .filter(|&(song, start)| {
                lists
                    .iter()
                    .enumerate()
                    .all(|(i, l)| l.binary_search(&(song, start + i)).is_ok())
            })
            .collect();
        found.dedup_by_key(|f| f.0);
        found
    }
}
