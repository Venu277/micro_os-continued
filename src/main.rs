#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(micro_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::println;
#[cfg(not(test))]
use micro_os::task::executor::Executor;
#[cfg(not(test))]
use micro_os::task::Task;
use x86_64::VirtAddr;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // 1. Read TSC as early as possible
    let tsc_start: u64 = unsafe { core::arch::x86_64::_rdtsc() };

    println!("==================================================");
    println!("   MicroOS v0.5 - Async Kernel & Interactive Shell");
    println!("==================================================");
    println!("Kernel booted successfully!");
    micro_os::serial_println!("Serial port online.");

    // 2. Initialize CPU structures (GDT, TSS, IDT, PIC, Interrupts enabled)
    micro_os::init();

    // 3. Initialize Memory Management (Paging, Frame Allocator, Kernel Heap)
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { micro_os::memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { micro_os::memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    micro_os::allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Kernel heap initialization failed");

    // 4. Measure elapsed TSC ticks up to shell prompt readiness
    let tsc_end: u64 = unsafe { core::arch::x86_64::_rdtsc() };
    let elapsed_ticks = tsc_end.wrapping_sub(tsc_start);

    // 5. Calibrate TSC frequency against PIT Channel 2
    let tsc_freq_hz = micro_os::interrupts::calibrate_tsc();
    let boot_time_ms = elapsed_ticks.saturating_mul(1000) / tsc_freq_hz;
    let boot_time_us = (elapsed_ticks.saturating_mul(1_000_000) / tsc_freq_hz) % 1000;
    let freq_mhz = tsc_freq_hz / 1_000_000;

    micro_os::shell::set_boot_time(boot_time_ms, boot_time_us, freq_mhz);

    micro_os::serial_println!(
        "BOOT_TIME_MS={}.{:03} (TSC ticks={}, freq={} MHz)",
        boot_time_ms,
        boot_time_us,
        elapsed_ticks,
        freq_mhz
    );

    println!(
        "Boot time to shell: {}.{:03} ms (calibrated @ {} MHz)",
        boot_time_ms,
        boot_time_us,
        freq_mhz
    );
    let now = micro_os::rtc::read_rtc();
    println!("System Clock: {}", now);
    println!("Type 'help' for available commands.");
    println!();

    #[cfg(test)]
    test_main();

    #[cfg(not(test))]
    {
        let mut executor = Executor::new();
        executor.spawn(Task::new(micro_os::shell::run_shell()));
        executor.run();
    }

    #[cfg(test)]
    micro_os::hlt_loop();
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
