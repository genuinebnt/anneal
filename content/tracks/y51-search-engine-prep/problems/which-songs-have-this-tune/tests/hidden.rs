use solution::*;

#[test]
fn a_note_before_the_start_of_a_song() {
    let mut b = Songbook::new();
    b.add(&[5, 1, 1, 1, 1]);
    b.add(&[1, 5]);
    // 5 is the rare note; in song 0 it is at place 0, so no tune can have two notes before it
    check!("1 1 5", b.find(&[1, 1, 5]), Vec::new());
    check!("5 1 1", b.find(&[5, 1, 1]), vec![(0, 0)]);
    check!("1 5", b.find(&[1, 5]), vec![(1, 0)]);
}

#[test]
fn single_notes_empty_songs_and_extreme_values() {
    let mut b = Songbook::new();
    b.add(&[]);
    b.add(&[0, u32::MAX, 0]);
    check!("0", b.find(&[0]), vec![(1, 0), (1, 2)]);
    check!("max 0", b.find(&[u32::MAX, 0]), vec![(1, 1)]);
    check!("an empty book", Songbook::new().find(&[0]), Vec::new());
}

#[test]
fn the_right_notes_at_the_wrong_distances() {
    let mut b = Songbook::new();
    b.add(&[1, 2, 9, 3]);
    b.add(&[1, 9, 2, 3]);
    b.add(&[2, 1, 3]);
    check!("1 2 3", b.find(&[1, 2, 3]), Vec::new());
}

#[test]
fn matches_reading_every_song() {
    let mut rng = anneal_prelude::Rng::new(5102);
    for _ in 0..200 {
        let mut b = Songbook::new();
        let mut songs: Vec<Vec<u32>> = Vec::new();
        for _ in 0..rng.below(8) {
            let len = rng.below(15);
            let song: Vec<u32> = (0..len).map(|_| rng.below(3) as u32).collect();
            b.add(&song);
            songs.push(song);
        }
        for _ in 0..10 {
            let len = 1 + rng.below(4);
            let tune: Vec<u32> = (0..len).map(|_| rng.below(3) as u32).collect();
            let mut want = Vec::new();
            for (s, song) in songs.iter().enumerate() {
                for (start, window) in song.windows(len).enumerate() {
                    if window == tune.as_slice() {
                        want.push((s as u32, start));
                    }
                }
            }
            check!(format!("songs {songs:?}, tune {tune:?}"), b.find(&tune), want);
        }
    }
}

#[test]
fn many_songs_with_a_planted_tune() {
    let mut rng = anneal_prelude::Rng::new(5103);
    let mut b = Songbook::new();
    let mut want = Vec::new();
    for s in 0..200 {
        let mut song: Vec<u32> = (0..50).map(|_| rng.below(2) as u32).collect();
        if s % 40 == 3 {
            song[20..24].copy_from_slice(&[0, 1, 1, 9]);
            want.push((s as u32, 20));
        }
        b.add(&song);
    }
    check!("the rare note last", b.find(&[0, 1, 1, 9]), want);
}

#[test]
fn the_end_of_one_song_and_the_start_of_the_next() {
    let mut b = Songbook::new();
    b.add(&[4, 5, 6]);
    b.add(&[7, 8]);
    b.add(&[]);
    b.add(&[9]);
    for tune in [&[6, 7][..], &[8, 9], &[5, 6, 7, 8], &[6, 7, 8]] {
        check!(format!("{tune:?}"), b.find(tune), Vec::new());
    }
}

#[test]
fn a_tune_that_repeats_a_note() {
    let mut b = Songbook::new();
    b.add(&[2, 1, 2, 1, 2]);
    check!("2 1 2", b.find(&[2, 1, 2]), vec![(0, 0), (0, 2)]);
    check!("1 2 1 2", b.find(&[1, 2, 1, 2]), vec![(0, 1)]);
}

#[test]
fn a_tune_as_long_as_a_song() {
    let mut b = Songbook::new();
    b.add(&[3, 1, 4, 1, 5]);
    check!("the whole song", b.find(&[3, 1, 4, 1, 5]), vec![(0, 0)]);
}

#[test]
fn asking_between_additions() {
    let mut b = Songbook::new();
    b.add(&[1, 2]);
    check!("before", b.find(&[1, 2]), vec![(0, 0)]);
    b.add(&[0, 1, 2]);
    check!("after", b.find(&[1, 2]), vec![(0, 0), (1, 1)]);
}

/// A list that is either given in full or made up: entry i is `(i / per_song, i % per_song * step + shift)`.
struct Counted {
    len: usize,
    made: (usize, usize, usize),
    given: Option<Vec<(u32, usize)>>,
    reads: std::rc::Rc<std::cell::Cell<u64>>,
    budget: u64,
}

impl PlaceList for Counted {
    fn len(&self) -> usize {
        self.len
    }
    fn get(&self, i: usize) -> (u32, usize) {
        assert!(i < self.len, "get({i}) on a list of {}", self.len);
        self.reads.set(self.reads.get() + 1);
        assert!(self.reads.get() <= self.budget, "more than the budget of {} reads", self.budget);
        match &self.given {
            Some(places) => places[i],
            None => {
                let (per_song, step, shift) = self.made;
                ((i / per_song) as u32, i % per_song * step + shift)
            }
        }
    }
}

enum Spec {
    Made(usize, (usize, usize, usize)),
    Given(Vec<(u32, usize)>),
}

fn run(specs: Vec<Spec>) -> Vec<(u32, usize)> {
    let lengths: Vec<usize> = specs.iter().map(|s| match s { Spec::Made(n, _) => *n, Spec::Given(v) => v.len() }).collect();
    let shortest = lengths.iter().copied().min().unwrap_or(0);
    let longest = lengths.iter().copied().max().unwrap_or(0);
    let bits = (usize::BITS - longest.leading_zeros()) as u64;
    let budget = lengths.len() as u64 * (shortest as u64 + 1) * (2 * bits + 4);
    let reads = std::rc::Rc::new(std::cell::Cell::new(0));
    let lists: Vec<Counted> = specs
        .into_iter()
        .zip(lengths)
        .map(|(spec, len)| {
            let (made, given) = match spec {
                Spec::Made(_, made) => (made, None),
                Spec::Given(v) => ((1, 0, 0), Some(v)),
            };
            Counted { len, made, given, reads: reads.clone(), budget }
        })
        .collect();
    let refs: Vec<&dyn PlaceList> = lists.iter().map(|l| l as &dyn PlaceList).collect();
    starts(&refs)
}

#[test]
fn counted_lists_made_from_small_songs() {
    let mut rng = anneal_prelude::Rng::new(5112);
    for _ in 0..1_500 {
        let songs: Vec<Vec<u32>> = (0..rng.below(5))
            .map(|_| {
                let len = rng.below(30);
                (0..len).map(|_| if rng.below(12) == 0 { 2 } else { rng.below(2) as u32 }).collect()
            })
            .collect();
        let len = 1 + rng.below(4);
        let tune: Vec<u32> = (0..len).map(|_| rng.below(3) as u32).collect();
        let places = |note: u32| {
            let mut out = Vec::new();
            for (s, song) in songs.iter().enumerate() {
                out.extend((0..song.len()).filter(|&i| song[i] == note).map(|i| (s as u32, i)));
            }
            out
        };
        let mut want = Vec::new();
        for (s, song) in songs.iter().enumerate() {
            for (start, window) in song.windows(len).enumerate() {
                if window == tune.as_slice() {
                    want.push((s as u32, start));
                }
            }
        }
        let got = run(tune.iter().map(|&n| Spec::Given(places(n))).collect());
        check!(format!("songs {songs:?}, tune {tune:?}"), got, want);
    }
}

#[test]
fn a_rare_note_in_any_place_of_the_tune() {
    // in every song of 100 notes, "common" is at each place from 0 to 99
    let common = |_: usize| Spec::Made(1_000_000_000, (100, 1, 0));
    for rare in 0..4 {
        let mut specs: Vec<Spec> = (0..3).map(common).collect();
        // places of the rare note: in song 0 too early for some offsets, in song 5_000_000 at the end
        specs.insert(rare, Spec::Given(vec![(0, 1), (7, 50), (5_000_000, 99)]));
        let want: Vec<(u32, usize)> = [(0u32, 1usize), (7, 50), (5_000_000, 99)]
            .into_iter()
            .filter(|&(_, at)| at >= rare && at - rare + 3 < 100)
            .map(|(s, at)| (s, at - rare))
            .collect();
        check!(format!("rare note at {rare}"), run(specs), want);
    }
}

#[test]
fn nothing_in_a_list_and_no_lists() {
    check!("one note unknown", run(vec![Spec::Made(1_000_000_000, (100, 1, 0)), Spec::Given(vec![])]), Vec::new());
    check!("no notes", run(vec![]), Vec::new());
}
