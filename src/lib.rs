#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::boxed::Box;
use core::panic::PanicInfo;

#[cfg(test)]
use bootloader::{entry_point, BootInfo};

pub mod acpi;
pub mod allocator;
pub mod cpu;
pub mod fs;
pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod net;
pub mod pci;
pub mod rtc;
pub mod serial;
pub mod shell;
pub mod storage;
pub mod task;
pub mod vga_buffer;

/// Initialize kernel CPU structures: GDT, IDT, PIC, and enable interrupts.
pub fn init() {
    gdt::init();
    interrupts::init_idt();
    unsafe { interrupts::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
}

/// Probe ATA disks and mount the MicroFS filesystem.
pub fn init_storage() {
    use storage::ata::{AtaBus, AtaDrive, AtaDriveSelect};
    use storage::BlockDevice;

    let device: Box<dyn BlockDevice> = match AtaDrive::new(AtaBus::Primary, AtaDriveSelect::Slave) {
        Ok(drive) => {
            serial_println!(
                "ATA Primary Slave hard drive detected ({} sectors, ~{} MiB)",
                drive.sector_count(),
                (drive.sector_count() * 512) / (1024 * 1024)
            );
            Box::new(drive)
        }
        Err(err) => {
            serial_println!("ATA drive probe note: {}. Using 64 KiB RAM disk fallback.", err);
            Box::new(storage::RamDisk::new(64 * 1024))
        }
    };

    let mut vfs = fs::VFS.lock();
    vfs.set_device(device);

    if let Some(dev) = vfs.dev_mut() {
        if fs::MicroFs::read_superblock(dev).is_err() {
            serial_println!("Unformatted disk detected. Formatting with MicroFS...");
            if let Err(e) = micro_fs_format_dev(dev) {
                serial_println!("Disk format error: {}", e);
            } else {
                serial_println!("MicroFS successfully formatted.");
            }
        } else {
            serial_println!("MicroFS superblock mounted successfully.");
        }
    }
}

fn micro_fs_format_dev(dev: &mut dyn storage::BlockDevice) -> Result<(), &'static str> {
    fs::MicroFs::format(dev)
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

// ---------- Testing Support ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;
    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

pub trait Testable {
    fn run(&self);
}

impl<T: Fn()> Testable for T {
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

#[cfg(test)]
entry_point!(test_kernel_main);

#[cfg(test)]
fn test_kernel_main(boot_info: &'static BootInfo) -> ! {
    init();
    use x86_64::VirtAddr;
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    init_storage();

    test_main();
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}

#[test_case]
fn test_box_allocation() {
    let heap_val = Box::new(42);
    assert_eq!(*heap_val, 42);
}

#[test_case]
fn test_vec_allocation() {
    use alloc::vec::Vec;
    let mut vec = Vec::new();
    for i in 0..100 {
        vec.push(i);
    }
    assert_eq!(vec.len(), 100);
    assert_eq!(vec[50], 50);
}
