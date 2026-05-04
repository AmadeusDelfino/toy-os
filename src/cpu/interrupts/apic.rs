use core::ptr::{read_volatile, write_volatile};

use x86_64::VirtAddr;

const LAPIC_ID_OFFSET: u32 = 0x20;
const LAPIC_ID_SHIFT: u32 = 24;
const LAPIC_VERSION_OFFSET: u32 = 0x30;

#[derive(Debug)]
pub struct APIC {
    mimo_mem_virt_addr: VirtAddr,
}

impl APIC {
    pub fn new(mimo_mem_virt_addr: VirtAddr) -> Self {
        Self { mimo_mem_virt_addr }
    }

    pub fn read(&self, offset: u32) -> u32 {
        unsafe { read_volatile((self.mimo_mem_virt_addr + offset as u64).as_ptr() as *const u32) }
    }

    pub fn write(&self, offset: u32, bytes: u32) {
        unsafe {
            write_volatile((self.mimo_mem_virt_addr + offset as u64).as_mut_ptr() as *mut u32, bytes);
        }
    }

    pub fn id(&self) -> u32 {
        (self.read(LAPIC_ID_OFFSET) >> LAPIC_ID_SHIFT) & 0xFF
    }

    pub fn version(&self) -> u32 {
        self.read(LAPIC_VERSION_OFFSET) & 0xFF
    }
}
