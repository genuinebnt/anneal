use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sale {
    pub id: u64,
    pub seats: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub version: u64,
    pub left: u32,
    /// Every sale in the register: id to seats.
    pub sales: BTreeMap<u64, u32>,
}

/// The reply never arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lost;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddError {
    /// The version was not the expected one. Nothing was written.
    Moved,
    /// The sale may or may not have been written.
    Lost,
}

pub trait Register {
    fn read(&mut self) -> Result<View, Lost>;
    fn version(&mut self) -> Result<u64, Lost>;
    fn add(&mut self, expected: u64, sale: Sale) -> Result<(), AddError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sold {
    /// This call's `add` succeeded.
    Now,
    /// The register already held this sale.
    Already,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SellError {
    IdReused,
    NotEnough { left: u32 },
    GaveUp,
}

pub fn sell<R: Register>(register: &mut R, sale: Sale, attempts: u32) -> Result<Sold, SellError> {
    for _ in 0..attempts {
        let Ok(view) = register.read() else {
            continue;
        };
        if let Some(&seats) = view.sales.get(&sale.id) {
            return if seats == sale.seats {
                Ok(Sold::Already)
            } else {
                Err(SellError::IdReused)
            };
        }
        if view.left < sale.seats {
            if register.version() == Ok(view.version) {
                return Err(SellError::NotEnough { left: view.left });
            }
            continue;
        }
        if register.add(view.version, sale).is_ok() {
            return Ok(Sold::Now);
        }
    }
    Err(SellError::GaveUp)
}
