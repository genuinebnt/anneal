/// The average of comma-separated numbers, e.g. "1, 2, 3".
pub fn average(input: &str) -> Result<f64, String> {
    let nums: Vec<f64> = input
        .split(',')
        .map(|s| s.trim().parse().unwrap())
        .collect();
    Ok(nums.iter().sum::<f64>() / nums.len() as f64)
}
