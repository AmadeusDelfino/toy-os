use core::arch::x86_64::__cpuid_count;

/// 48-byte brand string from CPUID leaves 0x80000002 - 0x80000004.
#[derive(Debug)]
pub struct CpuBrandString {
    buf: [u8; 48],
    len: usize,
}

impl CpuBrandString {
    pub fn read() -> Option<Self> {
        let max_ext = __cpuid_count(0x80000000, 0).eax;
        // check ext leaves
        if max_ext < 0x80000004 {
            return None;
        }

        let mut buf = [0u8; 48];

        for i in 0u32..3 {
            let result = __cpuid_count(0x80000002 + i, 0)
                ;

            let offset = (i as usize) * 16;
            buf[offset..offset + 4].copy_from_slice(&result.eax.to_le_bytes());
            buf[offset + 4..offset + 8].copy_from_slice(&result.ebx.to_le_bytes());
            buf[offset + 8..offset + 12].copy_from_slice(&result.ecx.to_le_bytes());
            buf[offset + 12..offset + 16].copy_from_slice(&result.edx.to_le_bytes());
        }

        let len = buf.iter()
            .rposition(|&b| b != 0 && b != b' ')
            .map(|p| p + 1)
            .unwrap_or(0);

        Some(Self { buf, len })
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("unknown")
    }
}

#[derive(Debug)]
pub struct CpuTopology {
    pub threads_per_core: u16,
    pub logical_processors: u16,
    pub cores: u16,
    pub apic_enabled: bool,
}

impl CpuTopology {
    pub fn read() -> Option<Self> {
        let mut threads_per_core = 1u16;
        let mut logical_processors = 1u16;

        for sub_leaf in 0u32.. {
            let result = __cpuid_count(0xB, sub_leaf);


            let level_type = (result.ecx >> 8) & 0xFF;

            // level_type 0 = invalid
            if level_type == 0 {
                break;
            }

            let count = (result.ebx & 0xFFFF) as u16;

            match level_type {
                1 => threads_per_core = count,  // SMT
                2 => logical_processors = count, // Core
                _ => {}
            }
        }

        if logical_processors == 0 || threads_per_core == 0 {
            return None;
        }

        Some(Self {
            threads_per_core,
            logical_processors,
            cores: logical_processors / threads_per_core,
            apic_enabled: false,
        })
    }
}