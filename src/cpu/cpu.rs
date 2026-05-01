use crate::cpu::info::{CpuBrandString, CpuTopology};

pub struct CPU {
    pub brand: CpuBrandString,
    pub topology: CpuTopology,
}

impl CPU {
    pub fn new() -> Self {
        let brand = CpuBrandString::read().expect("Failed to read CPU brand string");
        let topology = CpuTopology::read().expect("Failed to read CPU topology");

        Self { brand, topology }
    }
}
