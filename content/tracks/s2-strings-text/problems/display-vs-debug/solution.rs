use std::fmt;

#[derive(Debug)]
pub struct Money {
    pub cents: i64,
    pub currency: &'static str,
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.cents < 0 { "-" } else { "" };
        let abs = self.cents.unsigned_abs();
        f.pad(&format!(
            "{sign}{}.{:02} {}",
            abs / 100,
            abs % 100,
            self.currency
        ))
    }
}
