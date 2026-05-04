use core::ptr::{read_volatile, write_volatile};

use x86_64::VirtAddr;

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
        (self.read(0x20) >> 24) & 0xFF
    }

    pub fn version(&self) -> u32 {
        self.read(0x30) & 0xFF
    }
}
