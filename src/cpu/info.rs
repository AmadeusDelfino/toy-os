use x86_64::registers::model_specific::Msr;

use crate::println;
use core::arch::x86_64::__cpuid_count;

const CPUID_EXTENDED_FEATURES_LEAF: u32 = 0x80000000;
const CPUID_BRAND_STRING_FIRST_LEAF: u32 = 0x80000002;
const CPUID_BRAND_STRING_LAST_LEAF: u32 = 0x80000004;
const CPUID_BRAND_STRING_LEAF_COUNT: u32 = 3;
const CPUID_BRAND_STRING_BYTE_LEN: usize = 48;
const CPUID_BRAND_STRING_BYTES_PER_LEAF: usize = 16;
const CPUID_REGISTER_BYTE_LEN: usize = 4;
const CPUID_DEFAULT_SUBLEAF: u32 = 0;

const CPUID_EXTENDED_TOPOLOGY_LEAF: u32 = 0xB;
const CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_SHIFT: u32 = 8;
const CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_MASK: u32 = 0xFF;
const CPUID_EXTENDED_TOPOLOGY_LOGICAL_PROCESSOR_COUNT_MASK: u32 = 0xFFFF;
const CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_INVALID: u32 = 0;
const CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_SMT: u32 = 1;
const CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_CORE: u32 = 2;

const CPUID_FEATURE_INFO_LEAF: u32 = 0x1;
const CPUID_FEATURE_INFO_SUBLEAF: u32 = 0;
const CPUID_FEATURE_INFO_EDX_APIC: u32 = 1 << 9;
const CPUID_FEATURE_INFO_ECX_X2APIC: u32 = 1 << 21;

const IA32_APIC_BASE_MSR: u32 = 0x1B;
const IA32_APIC_BASE_X2APIC_ENABLE: u64 = 1 << 10;
const IA32_APIC_BASE_APIC_GLOBAL_ENABLE: u64 = 1 << 11;
const IA32_APIC_BASE_IS_BSP: u64 = 1 << 8;
const IA32_APIC_BASE_XAPIC_BASE_MASK: u64 = 0xFFFF_F000;

/// 48-byte brand string from CPUID leaves 0x80000002 - 0x80000004.
#[derive(Debug)]
pub struct CpuBrandString {
    buf: [u8; CPUID_BRAND_STRING_BYTE_LEN],
    len: usize,
}

impl CpuBrandString {
    pub fn print(&self) {
        println!("CPU brand: {}", self.as_str());
    }

    pub fn read() -> Option<Self> {
        let max_ext = __cpuid_count(CPUID_EXTENDED_FEATURES_LEAF, CPUID_DEFAULT_SUBLEAF).eax;
        // check ext leaves
        if max_ext < CPUID_BRAND_STRING_LAST_LEAF {
            return None;
        }

        let mut buf = [0u8; CPUID_BRAND_STRING_BYTE_LEN];

        for i in 0u32..CPUID_BRAND_STRING_LEAF_COUNT {
            let result = __cpuid_count(CPUID_BRAND_STRING_FIRST_LEAF + i, CPUID_DEFAULT_SUBLEAF);

            let offset = (i as usize) * CPUID_BRAND_STRING_BYTES_PER_LEAF;
            buf[offset..offset + CPUID_REGISTER_BYTE_LEN].copy_from_slice(&result.eax.to_le_bytes());
            buf[offset + CPUID_REGISTER_BYTE_LEN..offset + CPUID_REGISTER_BYTE_LEN * 2]
                .copy_from_slice(&result.ebx.to_le_bytes());
            buf[offset + CPUID_REGISTER_BYTE_LEN * 2..offset + CPUID_REGISTER_BYTE_LEN * 3]
                .copy_from_slice(&result.ecx.to_le_bytes());
            buf[offset + CPUID_REGISTER_BYTE_LEN * 3..offset + CPUID_BRAND_STRING_BYTES_PER_LEAF]
                .copy_from_slice(&result.edx.to_le_bytes());
        }

        let len = buf.iter().rposition(|&b| b != 0 && b != b' ').map(|p| p + 1).unwrap_or(0);

        Some(Self { buf, len })
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("unknown")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CpuTopology {
    pub threads_per_core: u16,
    pub logical_processors: u16,
    pub cores: u16,
    pub is_bsp: bool,
    pub apic_supported: bool,
    pub x2apic_supported: bool,
    pub apic_enabled: bool,
    pub x2apic_enabled: bool,
}

impl CpuTopology {
    pub fn print(&self) {
        println!(
            "
Is BSP: {}

CPU topology
Cores ({})
Threads ({})
----------

APIC
Supported ({})
Enabled ({})
----------

x2APIC
Supported ({})
Enabled ({})
",
            self.is_bsp,
            self.cores,
            self.logical_processors,
            self.apic_supported,
            self.apic_enabled,
            self.x2apic_supported,
            self.x2apic_enabled
        );
    }

    pub fn get_apic_mmio_phys_address(self) -> u64 {
        let apic_base = unsafe { Msr::new(IA32_APIC_BASE_MSR).read() };
        apic_base & IA32_APIC_BASE_XAPIC_BASE_MASK
    }

    pub fn read() -> Option<Self> {
        let mut threads_per_core = 1u16;
        let mut logical_processors = 1u16;

        for sub_leaf in 0u32.. {
            let result = __cpuid_count(CPUID_EXTENDED_TOPOLOGY_LEAF, sub_leaf);

            let level_type =
                (result.ecx >> CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_SHIFT) & CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_MASK;

            // level_type 0 = invalid
            if level_type == CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_INVALID {
                break;
            }

            let count = (result.ebx & CPUID_EXTENDED_TOPOLOGY_LOGICAL_PROCESSOR_COUNT_MASK) as u16;

            match level_type {
                CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_SMT => threads_per_core = count,
                CPUID_EXTENDED_TOPOLOGY_LEVEL_TYPE_CORE => logical_processors = count,
                _ => {}
            }
        }

        if logical_processors == 0 || threads_per_core == 0 {
            return None;
        }

        // CPUID leaf 0x1 reports APIC support in EDX bit 9.
        // Build a mask with only bit 9 set: 1 << 9.
        // If EDX has bit 9 set, the AND result is non-zero.
        // If EDX has bit 9 clear, the AND result is zero.
        // x2apic follows the same logic, but check bit 21 in ecx
        let apic_support_info = __cpuid_count(CPUID_FEATURE_INFO_LEAF, CPUID_FEATURE_INFO_SUBLEAF);
        let apic_supported = (apic_support_info.edx & CPUID_FEATURE_INFO_EDX_APIC) != 0;
        let x2apic_supported = (apic_support_info.ecx & CPUID_FEATURE_INFO_ECX_X2APIC) != 0;

        let apic_base = unsafe { Msr::new(IA32_APIC_BASE_MSR).read() };
        let x2apic_enabled = (apic_base & IA32_APIC_BASE_X2APIC_ENABLE) != 0;
        let apic_enabled = (apic_base & IA32_APIC_BASE_APIC_GLOBAL_ENABLE) != 0;
        let is_bsp = (apic_base & IA32_APIC_BASE_IS_BSP) != 0;

        Some(Self {
            threads_per_core,
            logical_processors,
            apic_supported,
            x2apic_supported,
            apic_enabled,
            x2apic_enabled,
            is_bsp,
            cores: logical_processors / threads_per_core,
        })
    }
}
