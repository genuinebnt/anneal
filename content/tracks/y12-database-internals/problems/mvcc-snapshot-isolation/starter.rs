#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxId(pub u64);

#[derive(Debug, PartialEq, Eq)]
pub enum TxError {
    NotActive,
    Conflict,
}

#[derive(Default)]
pub struct Db {}

impl Db {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(&mut self) -> TxId {
        todo!()
    }

    pub fn get(&self, tx: TxId, key: &str) -> Result<Option<i64>, TxError> {
        todo!()
    }

    pub fn put(&mut self, tx: TxId, key: &str, value: i64) -> Result<(), TxError> {
        todo!()
    }

    pub fn delete(&mut self, tx: TxId, key: &str) -> Result<(), TxError> {
        todo!()
    }

    pub fn commit(&mut self, tx: TxId) -> Result<(), TxError> {
        todo!()
    }

    pub fn abort(&mut self, tx: TxId) -> Result<(), TxError> {
        todo!()
    }

    pub fn versions(&self) -> usize {
        todo!()
    }

    pub fn vacuum(&mut self) -> usize {
        todo!()
    }
}
