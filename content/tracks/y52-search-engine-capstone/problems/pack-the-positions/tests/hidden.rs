use solution::*;

fn posting(page: u32, positions: &[u32]) -> Posting {
    Posting {
        page,
        positions: positions.to_vec(),
    }
}

#[test]
fn first_gaps_may_be_zero() {
    let list = [posting(0, &[0]), posting(1, &[0, 1])];
    let bytes = vec![2, 0, 1, 0, 1, 2, 0, 1];
    check!("encode", encode(&list), bytes.clone());
    check!("decode", decode(&bytes), Ok(list.to_vec()));
}

#[test]
fn a_position_may_not_repeat() {
    check!("positions 4, 4", decode(&[1, 3, 2, 4, 0]), Err(CodecError::NotAscending));
}

#[test]
fn five_byte_numbers() {
    let max = [0xff, 0xff, 0xff, 0xff, 0x0f];
    let mut bytes = vec![1];
    bytes.extend(max);
    bytes.extend([1, 0]);
    check!("page u32::MAX", decode(&bytes), Ok(vec![posting(u32::MAX, &[0])]));
    for fifth in [0x10, 0x1f, 0x7f, 0x80, 0x8f, 0xff] {
        let bytes = [1, 0xff, 0xff, 0xff, 0xff, fifth, 0, 1, 0];
        check!(format!("fifth byte {fifth:#x}"), decode(&bytes), Err(CodecError::Overflow));
    }
}

#[test]
fn gaps_that_add_up_too_far() {
    let max = [0xff, 0xff, 0xff, 0xff, 0x0f];
    let mut pages = vec![2];
    pages.extend(max);
    pages.extend([1, 0, 1, 1, 0]);
    check!("page u32::MAX then gap 1", decode(&pages), Err(CodecError::Overflow));
    let mut places = vec![1, 0, 2];
    places.extend(max);
    places.push(1);
    check!("position u32::MAX then gap 1", decode(&places), Err(CodecError::Overflow));
}

#[test]
fn longer_than_needed_is_accepted() {
    check!("0 as 80 00", decode(&[0x80, 0x00]), Ok(vec![]));
    check!("1 posting with long numbers", decode(&[0x81, 0x00, 0x83, 0x80, 0x00, 0x01, 0x80, 0x00]), Ok(vec![posting(3, &[0])]));
}

#[test]
fn the_first_problem_from_the_left() {
    check!("repeat before cut", decode(&[3, 5, 1, 0, 0]), Err(CodecError::NotAscending));
    check!("no positions before trailing", decode(&[1, 3, 0, 9, 9]), Err(CodecError::NoPositions));
}

#[test]
fn counts_that_lie() {
    let huge = [0xff, 0xff, 0xff, 0xff, 0x0f];
    check!("postings", decode(&huge), Err(CodecError::Truncated));
    let mut places = vec![1, 7];
    places.extend(huge);
    check!("positions", decode(&places), Err(CodecError::Truncated));
    // the same two inputs again, counting what is asked of the allocator
    for input in [&huge[..], &places[..], &[1, 7, 0xff, 0xff, 0x03]] {
        let (result, used) = anneal_prelude::allocs(|| decode(input));
        check!(format!("{input:?}"), result, Err(CodecError::Truncated));
        check!(format!("{input:?}: under 100000 bytes asked for ({})", used.bytes), used.bytes < 100_000, true);
    }
}

#[test]
fn every_cut_of_a_good_list_is_truncated() {
    let bytes = encode(&[posting(1, &[2, 3, 500]), posting(70_000, &[0]), posting(70_001, &[9])]);
    for cut in 0..bytes.len() {
        check!(format!("first {cut} bytes"), decode(&bytes[..cut]), Err(CodecError::Truncated));
    }
}

fn random_list(rng: &mut anneal_prelude::Rng) -> Vec<Posting> {
    let mut list = Vec::new();
    let mut page = rng.below(3) as u32;
    for _ in 0..rng.below(12) {
        let mut positions = Vec::new();
        let mut at = rng.below(3) as u32;
        for _ in 0..1 + rng.below(6) {
            positions.push(at);
            let span = if rng.below(8) == 0 { 3_000_000 } else { 40 };
            at += 1 + rng.below(span) as u32;
        }
        list.push(Posting { page, positions });
        page += 1 + if rng.bool() { rng.below(100) } else { rng.below(1 << 20) } as u32;
    }
    list
}

/// The format written out the long way.
fn plain_encode(list: &[Posting]) -> Vec<u8> {
    fn number(mut n: u32, out: &mut Vec<u8>) {
        loop {
            let low = (n % 128) as u8;
            n /= 128;
            out.push(if n > 0 { low + 128 } else { low });
            if n == 0 {
                return;
            }
        }
    }
    let mut out = Vec::new();
    number(list.len() as u32, &mut out);
    for (i, p) in list.iter().enumerate() {
        number(p.page - if i == 0 { 0 } else { list[i - 1].page }, &mut out);
        number(p.positions.len() as u32, &mut out);
        for (j, &at) in p.positions.iter().enumerate() {
            number(at - if j == 0 { 0 } else { p.positions[j - 1] }, &mut out);
        }
    }
    out
}

#[test]
fn random_lists_give_these_exact_bytes_and_come_back() {
    let mut rng = anneal_prelude::Rng::new(5201);
    for _ in 0..500 {
        let list = random_list(&mut rng);
        let bytes = encode(&list);
        check!(format!("encode {list:?}"), bytes.clone(), plain_encode(&list));
        check!(format!("decode {bytes:?}"), decode(&bytes), Ok(list));
    }
}

#[test]
fn random_bytes_never_panic_and_what_is_accepted_is_a_valid_list() {
    let mut rng = anneal_prelude::Rng::new(5202);
    let interesting = [0u8, 1, 2, 3, 0x0f, 0x10, 0x7f, 0x80, 0xff];
    for _ in 0..30_000 {
        let len = rng.below(14);
        let bytes: Vec<u8> = (0..len).map(|_| *rng.pick(&interesting)).collect();
        if let Ok(list) = decode(&bytes) {
            let rising = list.windows(2).all(|w| w[0].page < w[1].page)
                && list.iter().all(|p| !p.positions.is_empty() && p.positions.windows(2).all(|w| w[0] < w[1]));
            check!(format!("{bytes:?} gave {list:?}"), rising, true);
        }
    }
}

#[test]
fn a_long_list() {
    let list: Vec<Posting> = (0..100_000).map(|i| posting(i * 3, &[i % 7, 90])).collect();
    check!("round trip", decode(&encode(&list)) == Ok(list.clone()), true);
}
