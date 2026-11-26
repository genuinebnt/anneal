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

    /// Avoids holding two locks at all: debit, release, then credit. Never deadlocks, never atomic.
    pub fn transfer(&self, from: usize, to: usize, amount: u64) -> Result<(), TransferError> {
        let source = self.account(from)?;
        let target = self.account(to)?;
        {
            let mut source = source.lock().unwrap();
            (self.probe)(Step::HoldingFirst);
            if *source < amount {
                return Err(TransferError::Insufficient);
            }
            *source -= amount;
        }
        std::thread::yield_now();
        *target.lock().unwrap() += amount;
        Ok(())
    }

    pub fn balance(&self, i: usize) -> Option<u64> {
        self.accounts.get(i).map(|a| *a.lock().unwrap())
    }

    pub fn total(&self) -> u64 {
        let mut held = Vec::with_capacity(self.accounts.len());
        let mut sum = 0;
        for (i, account) in self.accounts.iter().enumerate() {
            let guard = account.lock().unwrap();
            sum += *guard;
            held.push(guard);
            (self.probe)(Step::Counted(i));
        }
        sum
    }
}
