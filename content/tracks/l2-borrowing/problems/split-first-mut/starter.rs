/// `names[0]` is a namespace. Prefixes every other name with "<namespace>::" in place, unless it already starts
/// with exactly that. An empty slice is left alone.
pub fn qualify(names: &mut [String]) {
    for i in 1..names.len() {
        let ns = &names[0];
        let done = names[i]
            .strip_prefix(ns.as_str())
            .is_some_and(|rest| rest.starts_with("::"));
        if !done {
            names[i].insert_str(0, "::");
            names[i].insert_str(0, ns);
        }
    }
}

/// Seals every whole frame of `size` bytes (`size >= 2`) in `buf` and returns how many it sealed; bytes after
/// the last whole frame are left alone. In a frame, byte 0 is the key and the last byte the checksum: XOR every
/// byte in between with the key, then set the checksum to the wrapping sum of those new bytes.
pub fn seal_frames(buf: &mut [u8], size: usize) -> usize {
    let mut sealed = 0;
    for frame in buf.chunks_mut(size) {
        let key = &frame[0];
        let sum = &mut frame[size - 1];
        *sum = 0;
        for b in &mut frame[1..size - 1] {
            *b ^= *key;
            *sum = sum.wrapping_add(*b);
        }
        sealed += 1;
    }
    sealed
}

/// Swaps the first half of `v` with the last half in place. With an odd length the middle element stays:
/// [1, 2, 3, 4, 5] becomes [4, 5, 3, 1, 2].
pub fn swap_halves<T>(v: &mut [T]) {
    let half = v.len() / 2;
    let (front, back) = (&mut v[..half], &mut v[v.len() - half..]);
    front.swap_with_slice(back);
}
