#![no_std]
#![no_main]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::acpi::{verify_checksum, GenericAddress, RsdpDescriptor, SdtHeader, ACPI_INFO};
use micro_os::cpu::get_cpu_info;
use micro_os::storage::nvme::{NvmeCompletionEntry, NvmeDevice, NvmeSubmissionEntry};
use micro_os::storage::BlockDevice;
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    serial_print!("hardware::cpuid_features...\t");
    test_cpuid_features();
    serial_println!("[ok]");

    serial_print!("hardware::acpi_structures...\t");
    test_acpi_structures();
    serial_println!("[ok]");

    serial_print!("hardware::nvme_structures...\t");
    test_nvme_structures();
    serial_println!("[ok]");

    serial_print!("hardware::nvme_block_device...\t");
    test_nvme_block_device();
    serial_println!("[ok]");

    serial_print!("hardware::acpi_discovery...\t");
    test_acpi_discovery(phys_mem_offset);
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_cpuid_features() {
    let cpu = get_cpu_info();
    assert!(!cpu.vendor.is_empty(), "CPU vendor should not be empty");
    assert!(!cpu.brand.is_empty(), "CPU brand should not be empty");
    assert!(cpu.logical_cores >= 1, "At least 1 logical core expected");
    assert!(cpu.phys_addr_bits >= 32, "At least 32-bit physical addressing expected");
    assert!(cpu.virt_addr_bits >= 48, "At least 48-bit virtual addressing expected");

    // Standard x86_64 features
    assert!(cpu.features.contains(&"fpu"), "FPU required for x86_64");
    assert!(cpu.features.contains(&"tsc"), "TSC required for x86_64");
    assert!(cpu.features.contains(&"sse"), "SSE required for x86_64");
    assert!(cpu.features.contains(&"sse2"), "SSE2 required for x86_64");
}

fn test_acpi_structures() {
    assert_eq!(core::mem::size_of::<RsdpDescriptor>(), 36);
    assert_eq!(core::mem::size_of::<SdtHeader>(), 36);
    assert_eq!(core::mem::size_of::<GenericAddress>(), 12);

    // Test checksum verification
    let mut valid_table = [0x52, 0x53, 0x44, 0x20, 0x50, 0x54, 0x52, 0x20, 0x00];
    let sum: u8 = valid_table[..8].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    valid_table[8] = (256 - sum as u16) as u8;
    assert!(verify_checksum(&valid_table));

    // Corrupted checksum
    valid_table[8] = valid_table[8].wrapping_add(1);
    assert!(!verify_checksum(&valid_table));
}

fn test_nvme_structures() {
    assert_eq!(core::mem::size_of::<NvmeSubmissionEntry>(), 64);
    assert_eq!(core::mem::size_of::<NvmeCompletionEntry>(), 16);
}

fn test_nvme_block_device() {
    let mut dev = NvmeDevice::new_laptop_profile();
    assert_eq!(dev.sector_size(), 512);
    assert_eq!(dev.sector_count(), 1_000_000_000);

    // Unwritten sector should be zeros
    let mut read_buf = [0xFFu8; 512];
    assert!(dev.read_sector(42, &mut read_buf).is_ok());
    assert_eq!(read_buf, [0u8; 512]);

    // Write pattern to sector 42
    let mut write_buf = [0u8; 512];
    for (i, b) in write_buf.iter_mut().enumerate() {
        *b = (i % 256) as u8;
    }
    assert!(dev.write_sector(42, &write_buf).is_ok());

    // Read back and verify
    assert!(dev.read_sector(42, &mut read_buf).is_ok());
    assert_eq!(read_buf, write_buf);

    // Other sectors remain 0
    assert!(dev.read_sector(43, &mut read_buf).is_ok());
    assert_eq!(read_buf, [0u8; 512]);
}

fn test_acpi_discovery(phys_mem_offset: VirtAddr) {
    micro_os::acpi::init_acpi(phys_mem_offset);
    let lock = ACPI_INFO.lock();
    if let Some(ref info) = *lock {
        assert!(info.rsdp_found);
        assert!(info.num_cpu_cores >= 1);
        assert_eq!(info.local_apic_addr, 0xFEE00000);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
