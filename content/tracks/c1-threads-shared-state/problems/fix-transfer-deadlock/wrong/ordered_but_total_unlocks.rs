use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    HoldingFirst,
    Counted(usize),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferError {
    UnknownAccount(usize),
    Insufficient,
}

pub type Probe = Arc<dyn Fn(Step) + Send + Sync>;

pub struct Bank {
    accounts: Vec<Mutex<u64>>,
    probe: Probe,
}

impl Bank {
    pub fn new(balances: &[u64]) -> Self {
        Self::with_probe(balances, Arc::new(|_| {}))
    }

    pub fn with_probe(balances: &[u64], probe: Probe) -> Self {
        Bank {
            accounts: balances.iter().map(|&b| Mutex::new(b)).collect(),
            probe,
        }
    }

    fn account(&self, i: usize) -> Result<&Mutex<u64>, TransferError> {
        self.accounts.get(i).ok_or(TransferError::UnknownAccount(i))
    }

    pub fn transfer(&self, from: usize, to: usize, amount: u64) -> Result<(), TransferError> {
        let source = self.account(from)?;
        let target = self.account(to)?;
        if from == to {
            let balance = source.lock().unwrap();
            (self.probe)(Step::HoldingFirst);
            return if *balance < amount {
                Err(TransferError::Insufficient)
            } else {
                Ok(())
            };
        }
        let (low, high) = if from < to {
            (source, target)
        } else {
            (target, source)
        };
        let low = low.lock().unwrap();
        (self.probe)(Step::HoldingFirst);
        let high = high.lock().unwrap();
        let (mut source, mut target) = if from < to { (low, high) } else { (high, low) };
        if *source < amount {
            return Err(TransferError::Insufficient);
        }
        *source -= amount;
        *target += amount;
        Ok(())
    }

    pub fn balance(&self, i: usize) -> Option<u64> {
        self.accounts.get(i).map(|a| *a.lock().unwrap())
    }

    pub fn total(&self) -> u64 {
        let mut sum = 0;
        for (i, account) in self.accounts.iter().enumerate() {
            sum += *account.lock().unwrap();
            (self.probe)(Step::Counted(i));
        }
        sum
    }
}
