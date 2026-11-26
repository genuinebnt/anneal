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
                // 0, then half, then ever smaller steps towards zero; the last step has size 1.
                let sign = x.signum();
                let nudges =
                    std::iter::successors(Some((x.unsigned_abs() / 4).max(1) as i64), |&d| {
                        (d > 1).then_some(d / 2)
                    });
                let step = [0, x / 2]
                    .into_iter()
                    .chain(nudges.map(move |d| x - sign * d))
                    .filter(|&y| y != x)
                    .find(|&y| {
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
