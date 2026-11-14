//! MicroOS ACPI (Advanced Configuration and Power Interface) Subsystem
//!
//! Parses ACPI tables (RSDP, XSDT/RSDT, FADT, MADT) to discover system topology,
//! Local APIC / I/O APIC addresses, CPU cores, and hardware power/reset controls.

use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::read_unaligned;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::VirtAddr;

use crate::serial_println;

/// ACPI 1.0 / 2.0 Root System Description Pointer (RSDP).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct RsdpDescriptor {
    pub signature: [u8; 8], // "RSD PTR "
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8, // 0 = ACPI 1.0, 2 = ACPI 2.0+
    pub rsdt_address: u32,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
    pub reserved: [u8; 3],
}

/// Standard ACPI System Description Table (SDT) Header (36 bytes).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SdtHeader {
    pub signature: [u8; 4],
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub oem_table_id: [u8; 8],
    pub oem_revision: u32,
    pub creator_id: u32,
    pub creator_revision: u32,
}

/// ACPI Generic Address Structure (GAS).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct GenericAddress {
    pub address_space: u8, // 0 = System Memory (MMIO), 1 = System I/O
    pub bit_width: u8,
    pub bit_offset: u8,
    pub access_size: u8,
    pub address: u64,
}

/// Fixed ACPI Description Table (FADT / "FACP").
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct FadtHeader {
    pub header: SdtHeader,
    pub firmware_ctrl: u32,
    pub dsdt: u32,
    pub reserved1: u8,
    pub preferred_pm_profile: u8,
    pub sci_interrupt: u16,
    pub smi_command_port: u32,
    pub acpi_enable: u8,
    pub acpi_disable: u8,
    pub s4bios_req: u8,
    pub pstate_control: u8,
    pub pm1a_event_block: u32,
    pub pm1b_event_block: u32,
    pub pm1a_control_block: u32,
    pub pm1b_control_block: u32,
    pub pm2_control_block: u32,
    pub pm_timer_block: u32,
    pub gpe0_block: u32,
    pub gpe1_block: u32,
    pub pm1_event_len: u8,
    pub pm1_control_len: u8,
    pub pm2_control_len: u8,
    pub pm_timer_len: u8,
    pub gpe0_len: u8,
    pub gpe1_len: u8,
    pub gpe1_base: u8,
    pub cstate_control: u8,
    pub worst_c2_latency: u16,
    pub worst_c3_latency: u16,
    pub flush_size: u16,
    pub flush_stride: u16,
    pub duty_offset: u8,
    pub duty_width: u8,
    pub day_alarm: u8,
    pub month_alarm: u8,
    pub century: u8,
    pub iapc_boot_arch: u16,
    pub reserved2: u8,
    pub flags: u32,
    pub reset_reg: GenericAddress,
    pub reset_value: u8,
}

/// Multiple APIC Description Table (MADT / "APIC").
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtHeader {
    pub header: SdtHeader,
    pub local_apic_address: u32,
    pub flags: u32,
}

/// Parsed ACPI summary for runtime hardware management and shell inspection.
#[derive(Debug, Clone)]
pub struct AcpiInfo {
    pub rsdp_found: bool,
    pub rsdp_phys: u64,
    pub revision: u8,
    pub oem_id: String,
    pub local_apic_addr: u64,
    pub io_apic_addr: u64,
    pub num_cpu_cores: usize,
    pub cpu_apic_ids: Vec<u8>,
    pub pm1a_cnt_blk: u16,
    pub reset_supported: bool,
    pub reset_port: u16,
    pub reset_val: u8,
}

pub static ACPI_INFO: Mutex<Option<AcpiInfo>> = Mutex::new(None);

/// Verify standard ACPI byte checksum (sum of all bytes modulo 256 is 0).
pub fn verify_checksum(slice: &[u8]) -> bool {
    let sum: u8 = slice.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    sum == 0
}

/// Scan physical memory for the ACPI Root System Description Pointer (RSDP).
pub fn init_acpi(phys_mem_offset: VirtAddr) {
    serial_println!("Scanning physical memory for ACPI RSDP descriptor...");

    // Search EBDA (0x80000 - 0x9FFFF) and BIOS ROM (0xE0000 - 0xFFFFF)
    let search_regions = [(0x80000usize, 0xA0000usize), (0xE0000usize, 0x100000usize)];

    let mut found_rsdp: Option<(RsdpDescriptor, u64)> = None;

    for (start, end) in search_regions {
        let mut cur = start;
        while cur < end {
            let virt = phys_mem_offset.as_u64() + cur as u64;
            let slice = unsafe { core::slice::from_raw_parts(virt as *const u8, 8) };
            if slice == b"RSD PTR " {
                let rsdp_slice = unsafe { core::slice::from_raw_parts(virt as *const u8, 20) };
                if verify_checksum(rsdp_slice) {
                    let rsdp: RsdpDescriptor = unsafe { read_unaligned(virt as *const RsdpDescriptor) };
                    found_rsdp = Some((rsdp, cur as u64));
                    break;
                }
            }
            cur += 16;
        }
        if found_rsdp.is_some() {
            break;
        }
    }

    let (rsdp, rsdp_phys) = match found_rsdp {
        Some(val) => val,
        None => {
            serial_println!("ACPI: No valid RSDP found in physical BIOS memory.");
            return;
        }
    };

    let oem_id = core::str::from_utf8(&rsdp.oem_id)
        .unwrap_or("UNKNOWN")
        .trim()
        .into();

    serial_println!(
        "ACPI: RSDP found at phys {:#x} (Revision {}, OEM: '{}')",
        rsdp_phys,
        rsdp.revision,
        oem_id
    );

    let mut info = AcpiInfo {
        rsdp_found: true,
        rsdp_phys,
        revision: rsdp.revision,
        oem_id,
        local_apic_addr: 0xFEE00000,
        io_apic_addr: 0xFEC00000,
        num_cpu_cores: 1,
        cpu_apic_ids: alloc::vec![0],
        pm1a_cnt_blk: 0x604,
        reset_supported: false,
        reset_port: 0x64,
        reset_val: 0xFE,
    };

    // Parse RSDT or XSDT
    let sdt_tables: Vec<u64> = if rsdp.revision >= 2 && rsdp.xsdt_address != 0 {
        parse_xsdt(phys_mem_offset, rsdp.xsdt_address)
    } else {
        parse_rsdt(phys_mem_offset, rsdp.rsdt_address as u64)
    };

    for sdt_phys in sdt_tables {
        let virt = phys_mem_offset.as_u64() + sdt_phys;
        let header: SdtHeader = unsafe { read_unaligned(virt as *const SdtHeader) };
        let sig = core::str::from_utf8(&header.signature).unwrap_or("");

        if sig == "FACP" {
            let fadt: FadtHeader = unsafe { read_unaligned(virt as *const FadtHeader) };
            info.pm1a_cnt_blk = fadt.pm1a_control_block as u16;
            if (fadt.flags & (1 << 10)) != 0 && fadt.reset_reg.address_space == 1 {
                info.reset_supported = true;
                info.reset_port = fadt.reset_reg.address as u16;
                info.reset_val = fadt.reset_value;
            }
            serial_println!(
                "ACPI: FADT detected. PM1a_CNT={:#x}, ResetPort={:#x}",
                info.pm1a_cnt_blk,
                info.reset_port
            );
        } else if sig == "APIC" {
            let madt: MadtHeader = unsafe { read_unaligned(virt as *const MadtHeader) };
            info.local_apic_addr = madt.local_apic_address as u64;

            let length = header.length as usize;
            let _madt_body_len = length.saturating_sub(core::mem::size_of::<MadtHeader>());
            let mut offset = core::mem::size_of::<MadtHeader>();
            let mut cores = 0;
            let mut apic_ids = Vec::new();

            while offset + 2 <= length {
                let entry_type = unsafe { *(virt as *const u8).add(offset) };
                let entry_len = unsafe { *(virt as *const u8).add(offset + 1) } as usize;
                if entry_len == 0 {
                    break;
                }

                if entry_type == 0 && entry_len >= 8 {
                    // Processor Local APIC
                    let apic_id = unsafe { *(virt as *const u8).add(offset + 3) };
                    let flags = unsafe { read_unaligned((virt as *const u8).add(offset + 4) as *const u32) };
                    if (flags & 1) != 0 || (flags & 2) != 0 {
                        cores += 1;
                        apic_ids.push(apic_id);
                    }
                } else if entry_type == 1 && entry_len >= 12 {
                    // I/O APIC
                    let io_apic_phys = unsafe {
                        read_unaligned((virt as *const u8).add(offset + 4) as *const u32)
                    };
                    info.io_apic_addr = io_apic_phys as u64;
                }

                offset += entry_len;
            }

            if cores > 0 {
                info.num_cpu_cores = cores;
                info.cpu_apic_ids = apic_ids;
            }

            serial_println!(
                "ACPI: MADT detected. Local APIC={:#x}, Cores discovered={}",
                info.local_apic_addr,
                info.num_cpu_cores
            );
        }
    }

    *ACPI_INFO.lock() = Some(info);
}

fn parse_rsdt(phys_mem_offset: VirtAddr, rsdt_phys: u64) -> Vec<u64> {
    let virt = phys_mem_offset.as_u64() + rsdt_phys;
    let header: SdtHeader = unsafe { read_unaligned(virt as *const SdtHeader) };
    let entry_count = (header.length as usize - core::mem::size_of::<SdtHeader>()) / 4;
    let mut tables = Vec::with_capacity(entry_count);

    let entries_ptr = (virt + core::mem::size_of::<SdtHeader>() as u64) as *const u32;
    for i in 0..entry_count {
        let addr = unsafe { read_unaligned(entries_ptr.add(i)) };
        tables.push(addr as u64);
    }
    tables
}

fn parse_xsdt(phys_mem_offset: VirtAddr, xsdt_phys: u64) -> Vec<u64> {
    let virt = phys_mem_offset.as_u64() + xsdt_phys;
    let header: SdtHeader = unsafe { read_unaligned(virt as *const SdtHeader) };
    let entry_count = (header.length as usize - core::mem::size_of::<SdtHeader>()) / 8;
    let mut tables = Vec::with_capacity(entry_count);

    let entries_ptr = (virt + core::mem::size_of::<SdtHeader>() as u64) as *const u64;
    for i in 0..entry_count {
        let addr = unsafe { read_unaligned(entries_ptr.add(i)) };
        tables.push(addr);
    }
    tables
}

/// Reset the machine using ACPI FADT hardware reset registers if supported,
/// with fallback to 8042 keyboard controller reset pulse.
pub fn acpi_reboot() -> ! {
    let lock = ACPI_INFO.lock();
    if let Some(ref info) = *lock {
        if info.reset_supported && info.reset_port != 0 {
            serial_println!(
                "ACPI: Triggering hardware reboot via I/O port {:#x} (value {:#x})...",
                info.reset_port,
                info.reset_val
            );
            unsafe {
                Port::<u8>::new(info.reset_port).write(info.reset_val);
            }
        }
    }
    drop(lock);

    // Fallback: 8042 reset
    unsafe {
        Port::<u8>::new(0x64).write(0xFE);
    }

    crate::hlt_loop();
}

/// Power off the machine using ACPI sleep control registers.
pub fn acpi_poweroff() -> ! {
    let lock = ACPI_INFO.lock();
    if let Some(ref info) = *lock {
        if info.pm1a_cnt_blk != 0 {
            serial_println!(
                "ACPI: Triggering power-off via PM1a control port {:#x}...",
                info.pm1a_cnt_blk
            );
            unsafe {
                // Sleep type 5 (S5 soft-off) | SLP_EN (bit 13 = 0x2000)
                Port::<u16>::new(info.pm1a_cnt_blk).write(0x2000 | (5 << 10));
            }
        }
    }
    drop(lock);

    // Fallback: QEMU / Bochs ACPI power-off ports
    unsafe {
        Port::<u16>::new(0x604).write(0x2000);
        Port::<u16>::new(0x4004).write(0x2000);
        Port::<u16>::new(0xB004).write(0x2000);
        Port::<u32>::new(0xf4).write(0x10);
    }

    crate::hlt_loop();
}
