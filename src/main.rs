#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(micro_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::println;
use x86_64::VirtAddr;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // 1. Read TSC as early as possible
    let tsc_start: u64 = unsafe { core::arch::x86_64::_rdtsc() };

    println!("==============================");
    println!("   MicroOS v0.4 - written in Rust");
    println!("==============================");
    println!("Kernel booted successfully!");
    micro_os::serial_println!("Serial port works too!");

    // 2. Initialize CPU structures (GDT, TSS, IDT, PIC, Interrupts enabled)
    micro_os::init();

    // 3. Initialize Memory Management (Paging, Frame Allocator, Kernel Heap)
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { micro_os::memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { micro_os::memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    micro_os::allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Kernel heap initialization failed");

    // 4. Measure elapsed TSC ticks up to heap readiness
    let tsc_end: u64 = unsafe { core::arch::x86_64::_rdtsc() };
    let elapsed_ticks = tsc_end.wrapping_sub(tsc_start);

    // 5. Calibrate TSC frequency against PIT Channel 2
    let tsc_freq_hz = micro_os::interrupts::calibrate_tsc();
    let boot_time_ms = elapsed_ticks.saturating_mul(1000) / tsc_freq_hz;
    let boot_time_us = (elapsed_ticks.saturating_mul(1_000_000) / tsc_freq_hz) % 1000;

    micro_os::serial_println!(
        "BOOT_TIME_MS={}.{:03} (TSC ticks={}, freq={} MHz)",
        boot_time_ms,
        boot_time_us,
        elapsed_ticks,
        tsc_freq_hz / 1_000_000
    );

    println!(
        "Boot time: {}.{:03} ms (calibrated @ {} MHz)",
        boot_time_ms,
        boot_time_us,
        tsc_freq_hz / 1_000_000
    );
    println!("GDT, TSS, IDT, PIC, Paging, and Heap (100 KiB) initialized.");

    // 6. Demonstrate dynamic heap allocations (Box, Vec, String)
    let test_box = Box::new(42);
    let mut test_vec = Vec::new();
    for i in 0..10 {
        test_vec.push(i * 2);
    }
    let test_string = String::from("MicroOS Dynamic Heap Allocator Online!");

    println!("Heap test: Box = {}, Vec len = {}", *test_box, test_vec.len());
    println!("{}", test_string);
    micro_os::serial_println!("Heap test: Box = {}, Vec len = {}", *test_box, test_vec.len());
    micro_os::serial_println!("{}", test_string);

    // 7. Test breakpoint exception
    x86_64::instructions::interrupts::int3();
    println!("Breakpoint handled — CPU did not crash!");
    micro_os::serial_println!("Kernel ready. Interrupts & Heap OK.");

    #[cfg(test)]
    test_main();

    #[cfg(not(test))]
    run_keyboard_loop();

    #[cfg(test)]
    micro_os::hlt_loop();
}

#[cfg(not(test))]
fn run_keyboard_loop() -> ! {
    use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    println!("Type on your keyboard (echo enabled):");

    loop {
        // Pop scancodes pushed by keyboard ISR into lock-free queue
        if let Some(scancode) = micro_os::interrupts::pop_scancode() {
            if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
                if let Some(key) = keyboard.process_keyevent(key_event) {
                    match key {
                        DecodedKey::Unicode(character) => {
                            micro_os::print!("{}", character);
                            micro_os::serial_print!("{}", character);
                        }
                        DecodedKey::RawKey(key) => {
                            micro_os::print!("{:?}", key);
                            micro_os::serial_print!("{:?}", key);
                        }
                    }
                }
            }
        } else {
            // Sleep CPU until next hardware interrupt (Timer IRQ0 or Keyboard IRQ1)
            x86_64::instructions::hlt();
        }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    micro_os::serial_println!("KERNEL PANIC: {}", info);
    micro_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
