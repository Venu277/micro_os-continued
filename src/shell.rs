use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::fs::vfs::VFS;
use crate::task::keyboard::ScancodeStream;
use crate::{print, println, serial_print, serial_println};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use x86_64::instructions::port::Port;

pub static BOOT_TIME_MS: AtomicU64 = AtomicU64::new(0);
pub static BOOT_TIME_US: AtomicU64 = AtomicU64::new(0);
pub static BOOT_TIME_FREQ_MHZ: AtomicU64 = AtomicU64::new(0);

pub fn set_boot_time(ms: u64, us: u64, freq_mhz: u64) {
    BOOT_TIME_MS.store(ms, Ordering::Relaxed);
    BOOT_TIME_US.store(us, Ordering::Relaxed);
    BOOT_TIME_FREQ_MHZ.store(freq_mhz, Ordering::Relaxed);
}

pub fn print_prompt() {
    let cwd = VFS.lock().current_path.clone();
    let prompt_str = if cwd.is_empty() { "/" } else { &cwd };
    print!("micro_os:{}> ", prompt_str);
    serial_print!("micro_os:{}> ", prompt_str);
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
        "bootime" => cmd_bootime(),
        "date" | "time" => cmd_date(),
        "lspci" => cmd_lspci(),
        "ls" => cmd_ls(&args),
        "cd" => cmd_cd(&args),
        "pwd" => cmd_pwd(),
        "mkdir" => cmd_mkdir(&args),
        "touch" => cmd_touch(&args),
        "cat" => cmd_cat(&args),
        "write" => cmd_write(&args),
        "rm" => cmd_rm(&args),
        "df" => cmd_df(),
        "mkfs" => cmd_mkfs(),
        "reboot" => cmd_reboot(),
        "shutdown" => cmd_shutdown(),
        unknown => {
            println!(
                "micro_os: unknown command: '{}'. Type 'help' for a list of commands.",
                unknown
            );
            serial_println!("micro_os: unknown command: '{}'", unknown);
        }
    }
}

fn cmd_help() {
    println!("MicroOS Shell - Available Commands:");
    println!("  help                 - Display this list of available commands");
    println!("  clear                - Clear the VGA display buffer");
    println!("  echo <msg>           - Print the given message to screen and serial");
    println!("  uptime               - Display system uptime and timer ticks");
    println!("  meminfo              - Show kernel heap memory usage statistics");
    println!("  bootime              - Show kernel initialization boot time");
    println!("  date                 - Display current CMOS RTC real-time clock");
    println!("  lspci                - List all discovered PCI devices");
    println!("  ls [path]            - List directory contents");
    println!("  cd <path>            - Change current working directory");
    println!("  pwd                  - Print working directory");
    println!("  mkdir <name>         - Create a new directory");
    println!("  touch <file>         - Create an empty file");
    println!("  write <file> <text>  - Write text into a file");
    println!("  cat <file>           - Display contents of a file");
    println!("  rm <name>            - Remove a file or empty directory");
    println!("  df                   - Show disk storage usage");
    println!("  mkfs                 - Format disk with clean MicroFS filesystem");
    println!("  reboot               - Restart the machine");
    println!("  shutdown             - Power off the system");
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

    println!(
        "System Uptime: {}h {}m {}s ({} PIT timer ticks)",
        hours, minutes, seconds, ticks
    );
    serial_println!(
        "System Uptime: {}h {}m {}s ({} ticks)",
        hours, minutes, seconds, ticks
    );
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
    serial_println!("Kernel Boot Time: {}.{:03} ms (@ {} MHz)", ms, us, freq);
}

fn cmd_date() {
    let now = crate::rtc::read_rtc();
    println!("CMOS RTC Clock: {}", now);
    serial_println!("CMOS RTC Clock: {}", now);
}

fn cmd_lspci() {
    let devices = crate::pci::enumerate_pci();
    println!("Discovered {} PCI device(s):", devices.len());
    serial_println!("PCI: {} device(s) found", devices.len());
    for dev in devices {
        println!(
            "  [{:02x}:{:02x}.{}] ID {:04x}:{:04x} - {}",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.class_name()
        );
        serial_println!(
            "  [{:02x}:{:02x}.{}] ID {:04x}:{:04x} - {}",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.class_name()
        );
    }
}

fn cmd_ls(args: &[&str]) {
    let path = if args.is_empty() { "" } else { args[0] };
    match VFS.lock().list_dir(path) {
        Ok(entries) => {
            println!("  TYPE   INODE   NAME");
            serial_println!("  TYPE   INODE   NAME");
            for entry in entries {
                let type_str = if entry.file_type == 2 { "DIR " } else { "FILE" };
                println!("  {}   {:04}    {}", type_str, entry.inode, entry.name_as_str());
                serial_println!("  {}   {:04}    {}", type_str, entry.inode, entry.name_as_str());
            }
        }
        Err(err) => {
            println!("ls: error: {}", err);
            serial_println!("ls: error: {}", err);
        }
    }
}

fn cmd_cd(args: &[&str]) {
    if args.is_empty() {
        let _ = VFS.lock().change_dir("/");
        return;
    }
    if let Err(err) = VFS.lock().change_dir(args[0]) {
        println!("cd: {}: {}", args[0], err);
        serial_println!("cd: {}: {}", args[0], err);
    }
}

fn cmd_pwd() {
    let cwd = VFS.lock().current_path.clone();
    println!("{}", cwd);
    serial_println!("{}", cwd);
}

fn cmd_mkdir(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: mkdir <directory_name>");
        return;
    }
    match VFS.lock().make_dir(args[0]) {
        Ok(()) => {
            println!("Created directory '{}'", args[0]);
            serial_println!("mkdir: created '{}'", args[0]);
        }
        Err(err) => {
            println!("mkdir: cannot create '{}': {}", args[0], err);
            serial_println!("mkdir: error: {}", err);
        }
    }
}

fn cmd_touch(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: touch <filename>");
        return;
    }
    match VFS.lock().touch(args[0]) {
        Ok(()) => {
            println!("Created file '{}'", args[0]);
            serial_println!("touch: created '{}'", args[0]);
        }
        Err(err) => {
            println!("touch: cannot create '{}': {}", args[0], err);
            serial_println!("touch: error: {}", err);
        }
    }
}

fn cmd_write(args: &[&str]) {
    if args.len() < 2 {
        println!("Usage: write <filename> <text...>");
        return;
    }
    let filename = args[0];
    let content = args[1..].join(" ");
    match VFS.lock().write_file(filename, content.as_bytes()) {
        Ok(bytes) => {
            println!("Wrote {} bytes to '{}'", bytes, filename);
            serial_println!("write: {} bytes to '{}'", bytes, filename);
        }
        Err(err) => {
            println!("write: error: {}", err);
            serial_println!("write: error: {}", err);
        }
    }
}

fn cmd_cat(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: cat <filename>");
        return;
    }
    match VFS.lock().read_file(args[0]) {
        Ok(data) => {
            if let Ok(text) = core::str::from_utf8(&data) {
                println!("{}", text);
                serial_println!("{}", text);
            } else {
                println!("[Binary data: {} bytes]", data.len());
                serial_println!("[Binary data: {} bytes]", data.len());
            }
        }
        Err(err) => {
            println!("cat: {}: {}", args[0], err);
            serial_println!("cat: {}: {}", args[0], err);
        }
    }
}

fn cmd_rm(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: rm <filename>");
        return;
    }
    match VFS.lock().remove(args[0]) {
        Ok(()) => {
            println!("Removed '{}'", args[0]);
            serial_println!("rm: removed '{}'", args[0]);
        }
        Err(err) => {
            println!("rm: cannot remove '{}': {}", args[0], err);
            serial_println!("rm: error: {}", err);
        }
    }
}

fn cmd_df() {
    match VFS.lock().disk_usage() {
        Ok((total_kb, used_kb, free_kb, total_inodes, free_inodes)) => {
            println!("MicroFS Storage Usage:");
            println!("  Total Space:   {} KiB ({} MiB)", total_kb, total_kb / 1024);
            println!("  Used Space:    {} KiB ({}%)", used_kb, (used_kb * 100) / total_kb);
            println!("  Free Space:    {} KiB", free_kb);
            println!("  Inodes Used:   {}/{}", total_inodes - free_inodes, total_inodes);
            serial_println!("df: {} KiB total, {} KiB used, {} KiB free", total_kb, used_kb, free_kb);
        }
        Err(err) => {
            println!("df: error reading filesystem stats: {}", err);
            serial_println!("df: error: {}", err);
        }
    }
}

fn cmd_mkfs() {
    println!("Formatting storage with MicroFS...");
    serial_println!("Formatting disk with MicroFS...");
    match VFS.lock().format() {
        Ok(()) => {
            println!("Filesystem formatted and mounted successfully.");
            serial_println!("mkfs: format complete.");
        }
        Err(err) => {
            println!("mkfs: format failed: {}", err);
            serial_println!("mkfs: error: {}", err);
        }
    }
}

fn cmd_reboot() -> ! {
    println!("Rebooting MicroOS...");
    serial_println!("Rebooting MicroOS via 8042 reset pulse...");

    unsafe {
        let mut port_64 = Port::<u8>::new(0x64);
        port_64.write(0xFE);

        // Fallback: Triple fault
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
        Port::<u16>::new(0x604).write(0x2000);
        Port::<u16>::new(0x4004).write(0x2000);
        Port::<u16>::new(0xB004).write(0x2000);
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

    #[test_case]
    fn test_storage_command_tokens() {
        let input = "write notes.txt hello_storage";
        let mut parts = input.split_whitespace();
        assert_eq!(parts.next(), Some("write"));
        assert_eq!(parts.next(), Some("notes.txt"));
        assert_eq!(parts.next(), Some("hello_storage"));
    }
}
