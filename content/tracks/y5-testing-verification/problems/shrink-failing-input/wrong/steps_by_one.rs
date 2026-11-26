pub fn shrink(input: Vec<i64>, mut fails: impl FnMut(&[i64]) -> bool) -> Vec<i64> {
    if !fails(&input) {
        return input;
    }
    let mut current = input;
    loop {
        let mut changed = false;
        // Remove blocks, large ones first.
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
        // Move each value towards zero, biggest jump first.
        for i in 0..current.len() {
            loop {
                let x = current[i];
                if x == 0 {
                    break;
                }
                let step = [0, x / 2, x - x.signum()].into_iter().find(|&y| {
                    current[i] = y;
                    let still = fails(&current);
                    current[i] = x;
                    still
                });
                let Some(y) = step else { break };
                current[i] = y;
                changed = true;
            }
        }
        if !changed {
            return current;
        }
    }
}
