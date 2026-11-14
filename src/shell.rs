use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::task::keyboard::ScancodeStream;
use crate::{print, println, serial_print, serial_println};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use x86_64::instructions::port::Port;

pub static BOOT_TIME_MS: AtomicU64 = AtomicU64::new(0);
pub static BOOT_TIME_US: AtomicU64 = AtomicU64::new(0);
pub static BOOT_TIME_FREQ_MHZ: AtomicU64 = AtomicU64::new(0);

/// Sets the boot time recorded during kernel initialization.
pub fn set_boot_time(ms: u64, us: u64, freq_mhz: u64) {
    BOOT_TIME_MS.store(ms, Ordering::Relaxed);
    BOOT_TIME_US.store(us, Ordering::Relaxed);
    BOOT_TIME_FREQ_MHZ.store(freq_mhz, Ordering::Relaxed);
}

pub fn print_prompt() {
    print!("micro_os> ");
    serial_print!("micro_os> ");
}

/// Asynchronous interactive shell loop.
pub async fn run_shell() {
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );
    let scancodes = ScancodeStream::new();
    let mut line_buffer = String::new();

    print_prompt();

    loop {
        let scancode = scancodes.next_scancode().await;
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(c) => match c {
                        '\n' | '\r' => {
                            println!();
                            serial_println!();
                            execute_command(&line_buffer);
                            line_buffer.clear();
                            print_prompt();
                        }
                        '\x08' => {
                            // Backspace
                            if !line_buffer.is_empty() {
                                line_buffer.pop();
                                crate::vga_buffer::backspace();
                                serial_print!("\x08 \x08");
                            }
                        }
                        c => {
                            line_buffer.push(c);
                            print!("{}", c);
                            serial_print!("{}", c);
                        }
                    },
                    DecodedKey::RawKey(_) => {}
                }
            }
        }
    }
}

pub fn execute_command(input: &str) {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return;
    }

    let mut parts = trimmed.split_whitespace();
    let command = parts.next().unwrap_or("");
    let args: Vec<&str> = parts.collect();

    match command {
        "help" => cmd_help(),
        "clear" => cmd_clear(),
        "echo" => cmd_echo(&args),
        "uptime" => cmd_uptime(),
        "meminfo" => cmd_meminfo(),
        "reboot" => cmd_reboot(),
        "shutdown" => cmd_shutdown(),
        "bootime" => cmd_bootime(),
        "date" | "time" => cmd_date(),
        unknown => {
            println!("micro_os: unknown command: '{}'. Type 'help' for a list of commands.", unknown);
            serial_println!("micro_os: unknown command: '{}'", unknown);
        }
    }
}

fn cmd_help() {
    println!("MicroOS Shell - Available Commands:");
    println!("  help       - Display this list of available commands");
    println!("  clear      - Clear the VGA display buffer");
    println!("  echo <msg> - Print the given message to screen and serial");
    println!("  uptime     - Display system uptime and timer ticks");
    println!("  meminfo    - Show kernel heap memory usage statistics");
    println!("  bootime    - Show kernel initialization boot time");
    println!("  date       - Display current CMOS RTC real-time clock");
    println!("  reboot     - Restart the machine");
    println!("  shutdown   - Power off the system");
    serial_println!("Shell command 'help' executed.");
}

fn cmd_clear() {
    crate::vga_buffer::clear_screen();
    serial_println!("Shell command 'clear' executed.");
}

fn cmd_echo(args: &[&str]) {
    let mut first = true;
    for arg in args {
        if !first {
            print!(" ");
            serial_print!(" ");
        }
        print!("{}", arg);
        serial_print!("{}", arg);
        first = false;
    }
    println!();
    serial_println!();
}

fn cmd_uptime() {
    let sec = crate::rtc::uptime_seconds();
    let hours = sec / 3600;
    let minutes = (sec % 3600) / 60;
    let seconds = sec % 60;
    let ticks = crate::interrupts::timer_ticks();

    println!("System Uptime: {}h {}m {}s ({} PIT timer ticks)", hours, minutes, seconds, ticks);
    serial_println!("System Uptime: {}h {}m {}s ({} ticks)", hours, minutes, seconds, ticks);
}

fn cmd_meminfo() {
    let (used, free, total) = crate::allocator::memory_usage();
    let pct = if total > 0 { (used * 100) / total } else { 0 };

    println!("Kernel Heap Memory Info:");
    println!("  Total Heap: {} bytes ({} KiB)", total, total / 1024);
    println!("  Used Heap:  {} bytes ({} KiB) [{}%]", used, used / 1024, pct);
    println!("  Free Heap:  {} bytes ({} KiB)", free, free / 1024);
    serial_println!("Heap: total={}B, used={}B, free={}B", total, used, free);
}

fn cmd_bootime() {
    let ms = BOOT_TIME_MS.load(Ordering::Relaxed);
    let us = BOOT_TIME_US.load(Ordering::Relaxed);
    let freq = BOOT_TIME_FREQ_MHZ.load(Ordering::Relaxed);
    println!(
        "Kernel Boot Time: {}.{:03} ms (TSC calibrated @ {} MHz)",
        ms, us, freq
    );
    serial_println!(
        "Kernel Boot Time: {}.{:03} ms (@ {} MHz)",
        ms, us, freq
    );
}

fn cmd_date() {
    let now = crate::rtc::read_rtc();
    println!("CMOS RTC Clock: {}", now);
    serial_println!("CMOS RTC Clock: {}", now);
}

fn cmd_reboot() -> ! {
    println!("Rebooting MicroOS...");
    serial_println!("Rebooting MicroOS via 8042 reset pulse...");

    unsafe {
        // Method 1: PS/2 8042 keyboard controller reset pulse (port 0x64, command 0xFE)
        let mut port_64 = Port::<u8>::new(0x64);
        port_64.write(0xFE);

        // Method 2 (fallback): Triple fault via zeroed IDT
        x86_64::instructions::tables::lidt(&x86_64::structures::DescriptorTablePointer {
            limit: 0,
            base: x86_64::VirtAddr::new(0),
        });
        x86_64::instructions::interrupts::int3();
    }

    crate::hlt_loop();
}

fn cmd_shutdown() -> ! {
    println!("Shutting down MicroOS. Goodbye!");
    serial_println!("Shutting down MicroOS via ACPI/QEMU ports...");

    unsafe {
        // QEMU ACPI power-off (newer QEMU)
        Port::<u16>::new(0x604).write(0x2000);
        // VirtualBox power-off
        Port::<u16>::new(0x4004).write(0x2000);
        // Bochs / older QEMU power-off
        Port::<u16>::new(0xB004).write(0x2000);
        // QEMU isa-debug-exit
        Port::<u32>::new(0xf4).write(0x10);
    }

    crate::hlt_loop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_command_parsing() {
        let input = "echo hello world 123";
        let parts: Vec<&str> = input.split_whitespace().collect();
        assert_eq!(parts[0], "echo");
        assert_eq!(parts[1], "hello");
        assert_eq!(parts[2], "world");
        assert_eq!(parts[3], "123");
    }
}
