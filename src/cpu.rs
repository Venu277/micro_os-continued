//! MicroOS CPU Identification & Hardware Topology Subsystem
//!
//! Enumerate CPU capabilities, manufacturer vendor strings, processor brand,
//! core topology, and feature flags (AVX2, x2APIC, TSC, RDRAND) via x86_64 CPUID.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::x86_64::{__cpuid, __cpuid_count};

/// Comprehensive CPU architecture and topology information.
#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub vendor: String,
    pub brand: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub logical_cores: u32,
    pub phys_addr_bits: u8,
    pub virt_addr_bits: u8,
    pub features: Vec<&'static str>,
}

/// Retrieve processor information via CPUID instructions.
pub fn get_cpu_info() -> CpuInfo {
    // 1. Vendor String (Leaf 0)
    let leaf0 = unsafe { __cpuid(0x0000_0000) };
    let max_leaf = leaf0.eax;
    let mut vendor_bytes = [0u8; 12];
    vendor_bytes[0..4].copy_from_slice(&leaf0.ebx.to_le_bytes());
    vendor_bytes[4..8].copy_from_slice(&leaf0.edx.to_le_bytes());
    vendor_bytes[8..12].copy_from_slice(&leaf0.ecx.to_le_bytes());
    let vendor = core::str::from_utf8(&vendor_bytes)
        .unwrap_or("Unknown")
        .trim()
        .into();

    // 2. Family, Model, Stepping & Basic Features (Leaf 1)
    let mut family = 0;
    let mut model = 0;
    let mut stepping = 0;
    let mut logical_cores = 1;
    let mut features = Vec::new();

    if max_leaf >= 1 {
        let leaf1 = unsafe { __cpuid(0x0000_0001) };
        stepping = leaf1.eax & 0x0F;
        let base_model = (leaf1.eax >> 4) & 0x0F;
        let base_family = (leaf1.eax >> 8) & 0x0F;
        let ext_model = (leaf1.eax >> 16) & 0x0F;
        let ext_family = (leaf1.eax >> 20) & 0xFF;

        family = if base_family == 15 {
            base_family + ext_family
        } else {
            base_family
        };

        model = if base_family == 6 || base_family == 15 {
            (ext_model << 4) | base_model
        } else {
            base_model
        };

        logical_cores = ((leaf1.ebx >> 16) & 0xFF).max(1);

        // Feature flags in EDX
        if (leaf1.edx & (1 << 0)) != 0 { features.push("fpu"); }
        if (leaf1.edx & (1 << 4)) != 0 { features.push("tsc"); }
        if (leaf1.edx & (1 << 9)) != 0 { features.push("apic"); }
        if (leaf1.edx & (1 << 15)) != 0 { features.push("cmov"); }
        if (leaf1.edx & (1 << 23)) != 0 { features.push("mmx"); }
        if (leaf1.edx & (1 << 25)) != 0 { features.push("sse"); }
        if (leaf1.edx & (1 << 26)) != 0 { features.push("sse2"); }

        // Feature flags in ECX
        if (leaf1.ecx & (1 << 0)) != 0 { features.push("sse3"); }
        if (leaf1.ecx & (1 << 9)) != 0 { features.push("ssse3"); }
        if (leaf1.ecx & (1 << 12)) != 0 { features.push("fma"); }
        if (leaf1.ecx & (1 << 19)) != 0 { features.push("sse4.1"); }
        if (leaf1.ecx & (1 << 20)) != 0 { features.push("sse4.2"); }
        if (leaf1.ecx & (1 << 21)) != 0 { features.push("x2apic"); }
        if (leaf1.ecx & (1 << 23)) != 0 { features.push("popcnt"); }
        if (leaf1.ecx & (1 << 25)) != 0 { features.push("aes"); }
        if (leaf1.ecx & (1 << 28)) != 0 { features.push("avx"); }
        if (leaf1.ecx & (1 << 30)) != 0 { features.push("rdrand"); }
        if (leaf1.ecx & (1 << 31)) != 0 { features.push("hypervisor"); }
    }

    // 3. Extended Features (Leaf 7, Subleaf 0)
    if max_leaf >= 7 {
        let leaf7 = unsafe { __cpuid_count(0x0000_0007, 0) };
        if (leaf7.ebx & (1 << 0)) != 0 { features.push("fsgsbase"); }
        if (leaf7.ebx & (1 << 3)) != 0 { features.push("bmi1"); }
        if (leaf7.ebx & (1 << 5)) != 0 { features.push("avx2"); }
        if (leaf7.ebx & (1 << 7)) != 0 { features.push("smep"); }
        if (leaf7.ebx & (1 << 8)) != 0 { features.push("bmi2"); }
        if (leaf7.ebx & (1 << 9)) != 0 { features.push("erms"); }
        if (leaf7.ebx & (1 << 18)) != 0 { features.push("rdseed"); }
        if (leaf7.ebx & (1 << 20)) != 0 { features.push("smap"); }
    }

    // 4. Extended Leaves (0x8000_0000+)
    let ext_leaf0 = unsafe { __cpuid(0x8000_0000) };
    let max_ext_leaf = ext_leaf0.eax;

    // Processor Brand String (Leaves 0x8000_0002 to 0x8000_0004 = 48 bytes)
    let mut brand_bytes = [0u8; 48];
    if max_ext_leaf >= 0x8000_0004 {
        for i in 0..3 {
            let res = unsafe { __cpuid(0x8000_0002 + i) };
            let offset = (i as usize) * 16;
            brand_bytes[offset..offset + 4].copy_from_slice(&res.eax.to_le_bytes());
            brand_bytes[offset + 4..offset + 8].copy_from_slice(&res.ebx.to_le_bytes());
            brand_bytes[offset + 8..offset + 12].copy_from_slice(&res.ecx.to_le_bytes());
            brand_bytes[offset + 12..offset + 16].copy_from_slice(&res.edx.to_le_bytes());
        }
    }

    let brand: String = core::str::from_utf8(&brand_bytes)
        .unwrap_or("")
        .trim_matches('\0')
        .trim()
        .into();

    let final_brand = if brand.is_empty() {
        format!("Generic x86_64 CPU (Family {} Model {})", family, model)
    } else {
        brand
    };

    // Address widths (Leaf 0x8000_0008)
    let mut phys_addr_bits = 36;
    let mut virt_addr_bits = 48;
    if max_ext_leaf >= 0x8000_0008 {
        let leaf8 = unsafe { __cpuid(0x8000_0008) };
        phys_addr_bits = (leaf8.eax & 0xFF) as u8;
        virt_addr_bits = ((leaf8.eax >> 8) & 0xFF) as u8;
    }

    CpuInfo {
        vendor,
        brand: final_brand,
        family,
        model,
        stepping,
        logical_cores,
        phys_addr_bits,
        virt_addr_bits,
        features,
    }
}
