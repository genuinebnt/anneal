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
    accounts: Mutex<Vec<u64>>,
    probe: Probe,
}

impl Bank {
    pub fn new(balances: &[u64]) -> Self {
        Self::with_probe(balances, Arc::new(|_| {}))
    }

    pub fn with_probe(balances: &[u64], probe: Probe) -> Self {
        Bank {
            accounts: Mutex::new(balances.to_vec()),
            probe,
        }
    }

    pub fn transfer(&self, from: usize, to: usize, amount: u64) -> Result<(), TransferError> {
        let mut accounts = self.accounts.lock().unwrap();
        for i in [from, to] {
            if i >= accounts.len() {
                return Err(TransferError::UnknownAccount(i));
            }
        }
        (self.probe)(Step::HoldingFirst);
        if accounts[from] < amount {
            return Err(TransferError::Insufficient);
        }
        accounts[from] -= amount;
        accounts[to] += amount;
        Ok(())
    }

    pub fn balance(&self, i: usize) -> Option<u64> {
        self.accounts.lock().unwrap().get(i).copied()
    }

    pub fn total(&self) -> u64 {
        let accounts = self.accounts.lock().unwrap();
        let mut sum = 0;
        for (i, a) in accounts.iter().enumerate() {
            sum += a;
            (self.probe)(Step::Counted(i));
        }
        sum
    }
}
