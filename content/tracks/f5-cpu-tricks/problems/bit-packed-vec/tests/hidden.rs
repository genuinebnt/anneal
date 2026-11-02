use solution::*;

#[test]
fn min_width() {
    check!(r#"width of 0, 1, 255, 256, u64::MAX"#, (PackedVec::min_width(&[]), PackedVec::min_width(&[0]), PackedVec::min_width(&[1]), PackedVec::min_width(&[3, 255]), PackedVec::min_width(&[256]), PackedVec::min_width(&[u64::MAX])), (1, 1, 1, 8, 9, 64));
}

#[test]
fn width_64_holds_everything() {
    check!(r#"u64::MAX and 0 side by side"#, { let v = PackedVec::from_slice(64, &[u64::MAX, 0, 1 << 63, 12345]).unwrap(); (v.iter().collect::<Vec<_>>(), v.words_used()) }, (vec![u64::MAX, 0, 1 << 63, 12345], 4));
}

#[test]
fn width_1_bits() {
    check!(r#"a bitmap of 70 flags"#, { let flags: Vec<u64> = (0..70).map(|i| (i % 3 == 0) as u64).collect(); let v = PackedVec::from_slice(1, &flags).unwrap(); (v.iter().collect::<Vec<_>>() == flags, v.words_used()) }, (true, 2));
}

#[test]
fn straddling_values() {
    check!(r#"width 13: values cross word boundaries"#, { let vals: Vec<u64> = (0..40).map(|i| (i * 977 + 5) % 8192).collect(); let v = PackedVec::from_slice(13, &vals).unwrap(); (v.iter().collect::<Vec<_>>() == vals, v.words_used()) }, (true, 9));
}

#[test]
fn set_on_a_straddling_slot() {
    check!(r#"width 13, slot 4 straddles words 0 and 1"#, { let mut v = PackedVec::from_slice(13, &[8191; 10]).unwrap(); v.set(4, 0).unwrap(); let a: Vec<u64> = v.iter().collect(); v.set(4, 4097).unwrap(); let b: Vec<u64> = v.iter().collect(); (a, b) }, (vec![8191, 8191, 8191, 8191, 0, 8191, 8191, 8191, 8191, 8191], vec![8191, 8191, 8191, 8191, 4097, 8191, 8191, 8191, 8191, 8191]));
}

#[test]
fn set_errors() {
    check!(r#"out of range and too wide, nothing changes"#, { let mut v = PackedVec::from_slice(4, &[1, 2]).unwrap(); let r = (v.set(2, 1), v.set(0, 16), v.set(1, 15)); (r, v.iter().collect::<Vec<_>>()) }, ((Err(PackError::OutOfRange), Err(PackError::TooWide), Ok(())), vec![1, 15]));
}

#[test]
fn width_zero_panics() {
    check!(r#"width 0 and 65"#, (std::panic::catch_unwind(|| PackedVec::new(0)).is_err(), std::panic::catch_unwind(|| PackedVec::new(65)).is_err()), (true, true));
}

#[test]
fn from_slice_rejects_wide_values() {
    check!(r#"second value too wide"#, PackedVec::from_slice(3, &[7, 8]).map(|v| v.len()), Err(PackError::TooWide));
}

#[test]
fn random_vs_vec_for_every_width() {
    let mut rng = anneal_prelude::Rng::new(6202);
    for width in 1..=64u32 {
        let max = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
        let mut v = PackedVec::new(width);
        let mut model: Vec<u64> = Vec::new();
        let n = rng.below(150);
        let pick = |rng: &mut anneal_prelude::Rng| -> u64 {
            match rng.below(4) {
                0 => 0,
                1 => max,
                _ => ((rng.below(1 << 30) as u64) << 34 ^ rng.below(1 << 30) as u64 * 0x9E37_79B9) & max,
            }
        };
        for _ in 0..n {
            let x = pick(&mut rng);
            v.push(x).unwrap();
            model.push(x);
        }
        for _ in 0..n {
            let i = rng.below(n.max(1));
            if i < n {
                let x = pick(&mut rng);
                v.set(i, x).unwrap();
                model[i] = x;
            }
        }
        check!(format!("width {width}, {n} values: contents"), v.iter().collect::<Vec<_>>(), model.clone());
        check!(format!("width {width}, {n} values: words used"), v.words_used(), (n * width as usize).div_ceil(64));
        check!(format!("width {width}: get(n)"), v.get(n), None);
    }
}

#[test]
fn two_million_ten_bit_values() {
    let vals: Vec<u64> = (0..2_000_000u64).map(|i| (i * 7919) % 1000).collect();
    let v = PackedVec::from_slice(PackedVec::min_width(&vals), &vals).unwrap();
    let sum: u64 = v.iter().sum();
    check!("2,000,000 values below 1000: width, words, sum", (v.width(), v.words_used(), sum), (10, 312_500, vals.iter().sum::<u64>()));
}
