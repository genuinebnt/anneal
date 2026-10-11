//! Reading an `EXPLAIN ANALYZE` back: which operator spent the time itself?

#[derive(Debug, Clone, PartialEq)]
pub struct OpTime {
    pub op: String,
    pub depth: usize,
    pub inclusive_ms: f64,
    pub exclusive_ms: f64,
}

/// The time each operator spent itself, from the inclusive times of an `EXPLAIN ANALYZE` text: its own time minus its direct children's, never below 0.
pub fn exclusive_times(analysis: &str) -> Vec<OpTime> {
    // @begin 3i-c5
    let mut ops: Vec<OpTime> = Vec::new();
    for line in analysis.lines() {
        if line.trim().is_empty() || line.starts_with("===") {
            continue;
        }
        let depth = (line.len() - line.trim_start().len()) / 2;
        let body = line.trim();
        let (op, inclusive) = if let Some(at) = body.rfind(" (rows=") {
            let stats = &body[at..];
            let ms = stats
                .find("time=")
                .map(|i| &stats[i + 5..])
                .and_then(|t| t.split("ms").next())
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(0.0);
            (body[..at].to_string(), ms)
        } else {
            (body.trim_end_matches(" (never executed)").to_string(), 0.0)
        };
        ops.push(OpTime { op, depth, inclusive_ms: inclusive, exclusive_ms: inclusive });
    }
    // subtract each node's children (the nodes after it that are exactly one level deeper, until the depth comes back)
    for i in 0..ops.len() {
        let mut children = 0.0;
        for j in i + 1..ops.len() {
            if ops[j].depth <= ops[i].depth {
                break;
            }
            if ops[j].depth == ops[i].depth + 1 {
                children += ops[j].inclusive_ms;
            }
        }
        ops[i].exclusive_ms = (ops[i].inclusive_ms - children).max(0.0);
    }
    ops
    //~ todo!("3i-c5: parse each line (indent, text, time); exclusive = inclusive minus the inclusive of the direct children, at least 0")
    // @end
}

/// The operator with the largest exclusive time (the first when equal), `None` when there are no operators.
pub fn slowest(analysis: &str) -> Option<String> {
    // @begin 3i-c5
    let mut best: Option<OpTime> = None;
    for op in exclusive_times(analysis) {
        if best.as_ref().is_none_or(|b| op.exclusive_ms > b.exclusive_ms) {
            best = Some(op);
        }
    }
    best.map(|b| b.op)
    //~ todo!("3i-c5: the operator with the largest exclusive time")
    // @end
}
