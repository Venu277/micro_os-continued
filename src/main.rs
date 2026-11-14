#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(micro_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
use micro_os::println;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Read TSC as early as possible
    let tsc_start: u64 = unsafe { core::arch::x86_64::_rdtsc() };

    println!("==============================");
    println!("   MicroOS v0.3 - written in Rust");
    println!("==============================");
    println!("Kernel booted successfully!");
    micro_os::serial_println!("Serial port works too!");

    // 2. Kernel initialization (GDT, TSS, IDT, PIC, Interrupts enabled)
    micro_os::init();

    // 3. Read TSC after init()
    let tsc_end: u64 = unsafe { core::arch::x86_64::_rdtsc() };
    let elapsed_ticks = tsc_end.wrapping_sub(tsc_start);

    // 4. Calibrate TSC frequency using PIT Channel 2
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
    println!("GDT, TSS, IDT, PIC initialized. Interrupts enabled.");

    // 5. Test breakpoint exception
    x86_64::instructions::interrupts::int3();
    println!("Breakpoint handled — CPU did not crash!");
    micro_os::serial_println!("Kernel ready. Interrupts OK.");

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
