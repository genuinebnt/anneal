/// The average of comma-separated numbers, e.g. "1, 2, 3".
pub fn average(input: &str) -> Result<f64, String> {
    if input.trim().is_empty() {
        return Err("no numbers".into());
    }
    let nums = input
        .split(',')
        .map(|s| {
            let s = s.trim();
            s.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .ok_or_else(|| format!("not a number: {s}"))
        })
        .collect::<Result<Vec<f64>, String>>()?;
    Ok(nums.iter().sum::<f64>() / nums.len() as f64)
}
