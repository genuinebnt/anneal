pub fn shrink(input: Vec<i64>, mut fails: impl FnMut(&[i64]) -> bool) -> Vec<i64> {
    if !fails(&input) {
        return input;
    }
    let mut current = input;
    loop {
        let mut changed = false;
        let mut size = (current.len() / 2).max(1);
        loop {
            let mut start = 0;
            while start < current.len() {
                let end = (start + size).min(current.len());
                let candidate = [&current[..start], &current[end..]].concat();
                if fails(&candidate) {
                    current = candidate;
                    changed = true;
                } else {
                    start += size;
                }
            }
            if size == 1 {
                break;
            }
            size /= 2;
        }
        for i in 0..current.len() {
            while current[i] != 0 {
                let x = current[i];
                current[i] = x / 2;
                if fails(&current) {
                    changed = true;
                } else {
                    current[i] = x;
                    break;
                }
            }
        }
        if !changed {
            return current;
        }
    }
}
