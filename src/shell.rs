use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use crate::fs::vfs::VFS;
use crate::task::keyboard::ScancodeStream;
use crate::{print, println, serial_print, serial_println};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

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
        "lscpu" => cmd_lscpu(),
        "acpi" => cmd_acpi(),
        "nvme" => cmd_nvme(&args),
        "ifconfig" => cmd_ifconfig(),
        "iwconfig" => cmd_iwconfig(),
        "iwlist" => cmd_iwlist(&args),
        "wifi" => cmd_wifi(&args),
        "wpa_supplicant" => cmd_wpa_supplicant(&args),
        "ping" => cmd_ping(&args),
        "curl" | "http-get" => cmd_curl(&args),
        "apt" | "apt-get" => cmd_apt(&args),
        "dpkg" => cmd_dpkg(&args),
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
    println!("  lscpu                - Display CPU architecture, model, and flags");
    println!("  acpi                 - Display ACPI tables (RSDP, FADT, MADT, APIC)");
    println!("  nvme [cmd]           - NVMe storage controller status and identify");
    println!("  ifconfig             - Display network interfaces (eth0, wlan0) status");
    println!("  iwconfig             - Display 802.11 wireless link parameters");
    println!("  iwlist [iface] scan  - Scan for nearby 802.11 wireless networks");
    println!("  wifi <cmd> [args...] - Manage WiFi (scan, connect, disconnect, status)");
    println!("  wpa_supplicant -s .. - Connect to WPA2/WPA3 network with 4-way handshake");
    println!("  ping <ip>            - Send ICMP echo requests to target host");
    println!("  curl <ip> [path]     - Perform simple HTTP 1.0 request");
    println!("  apt <update|list|..> - Manage MicroOS software packages");
    println!("  dpkg <-l|-s|-i>      - Low-level Debian package manager tool");
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

pub fn parse_ipv4(s: &str) -> Option<smoltcp::wire::Ipv4Address> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in s.split('.') {
        if count >= 4 {
            return None;
        }
        let val: u8 = part.parse().ok()?;
        octets[count] = val;
        count += 1;
    }
    if count == 4 {
        Some(smoltcp::wire::Ipv4Address::new(
            octets[0], octets[1], octets[2], octets[3],
        ))
    } else {
        None
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        alloc::format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        alloc::format!("{}.{} KiB", bytes / 1024, (bytes % 1024) / 100)
    } else {
        alloc::format!("{}.{} MiB", bytes / (1024 * 1024), (bytes % (1024 * 1024)) / 100_000)
    }
}

fn cmd_ifconfig() {
    let mut printed = false;
    let net_lock = crate::net::NETWORK.lock();
    if let Some(ref stack) = *net_lock {
        let info = stack.ifconfig();
        println!("{}", info);
        serial_println!("{}", info);
        printed = true;
    }

    let wifi_lock = crate::net::wifi::WIFI_ADAPTER.lock();
    if let Some(ref adapter) = *wifi_lock {
        use crate::net::wifi::WiFiDriver;
        let status = adapter.status();
        let mac = adapter.mac_address();
        let mac_str = alloc::format!(
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
        );
        let status_str = alloc::format!(
            "{:<9} Link encap:Ethernet (802.11)  HWaddr {}\n          WiFi State:{}  Tx-Power=20 dBm\n          RX packets:{} errors:0 dropped:0 overruns:0 frame:0\n          TX packets:{} errors:0 dropped:0 overruns:0 carrier:0\n          RX bytes:{} ({})  TX bytes:{} ({})",
            status.interface,
            mac_str,
            status.state,
            status.rx_packets,
            status.tx_packets,
            status.rx_bytes,
            format_bytes(status.rx_bytes),
            status.tx_bytes,
            format_bytes(status.tx_bytes)
        );
        println!("{}", status_str);
        serial_println!("{}", status_str);
        printed = true;
    }

    if !printed {
        println!("ifconfig: No network interfaces initialized.");
        serial_println!("ifconfig: No network interfaces initialized.");
    }
}

fn cmd_iwconfig() {
    let wifi_lock = crate::net::wifi::WIFI_ADAPTER.lock();
    if let Some(ref adapter) = *wifi_lock {
        use crate::net::wifi::WiFiDriver;
        let status = adapter.status();
        let essid_str = match &status.ssid {
            Some(s) => alloc::format!("\"{}\"", s),
            None => "off/any".to_owned(),
        };
        let bssid_str = match &status.bssid {
            Some(b) => alloc::format!("{}", b),
            None => "Not-Associated".to_owned(),
        };
        let freq_ghz_major = status.frequency_mhz / 1000;
        let freq_ghz_minor = status.frequency_mhz % 1000;

        let out = alloc::format!(
            "{:<9} IEEE 802.11  ESSID:{}\n          Mode:Managed  Frequency:{}.{:03} GHz  Access Point: {}\n          Bit Rate={} Mb/s   Tx-Power={} dBm\n          Retry short limit:7   RTS thr:off   Fragment thr:off\n          Power Management:on\n          Link Quality={}/{}  Signal level={} dBm\n          Rx invalid nwid:0  Rx invalid crypt:0  Rx invalid frag:0\n          Tx excessive retries:0  Invalid misc:0   Missed beacon:0",
            status.interface,
            essid_str,
            freq_ghz_major,
            freq_ghz_minor,
            bssid_str,
            status.bitrate_mbps,
            status.tx_power_dbm,
            status.link_quality.0,
            status.link_quality.1,
            status.rssi
        );
        println!("{}", out);
        serial_println!("{}", out);
    } else {
        println!("iwconfig: No wireless extensions.");
        serial_println!("iwconfig: No wireless extensions.");
    }
}

fn cmd_iwlist(args: &[&str]) {
    let mut wifi_lock = crate::net::wifi::WIFI_ADAPTER.lock();
    if let Some(ref mut adapter) = *wifi_lock {
        use crate::net::wifi::WiFiDriver;
        let is_scan = args.iter().any(|&a| a == "scan" || a == "scanning");
        if !is_scan && !args.is_empty() {
            println!("Usage: iwlist [interface] scan");
            return;
        }

        println!("wlan0     Scan completed :");
        serial_println!("wlan0     Scan completed :");
        let aps = adapter.scan();
        for (i, ap) in aps.iter().enumerate() {
            let freq_major = ap.frequency_mhz() / 1000;
            let freq_minor = ap.frequency_mhz() % 1000;
            let quality = ap.link_quality();
            let key_str = match ap.auth {
                crate::net::wifi::WiFiAuthType::Open => "off (Open)",
                crate::net::wifi::WiFiAuthType::Wpa2Psk => "on (WPA2-PSK)",
                crate::net::wifi::WiFiAuthType::Wpa3Sae => "on (WPA3-SAE)",
            };

            let entry = alloc::format!(
                "          Cell {:02} - Address: {}\n                    Channel:{}\n                    Frequency:{}.{:03} GHz\n                    Quality={}/{}  Signal level={} dBm\n                    Encryption key:{}\n                    ESSID:\"{}\"\n                    Bit Rates:{} Mb/s",
                i + 1,
                ap.bssid,
                ap.channel,
                freq_major,
                freq_minor,
                quality.0,
                quality.1,
                ap.rssi,
                key_str,
                ap.ssid,
                ap.bitrate_mbps
            );
            println!("{}", entry);
            serial_println!("{}", entry);
        }
    } else {
        println!("iwlist: No wireless extensions.");
        serial_println!("iwlist: No wireless extensions.");
    }
}

fn cmd_wifi(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: wifi <scan|connect|disconnect|status> [args...]");
        println!("  wifi scan                    - Scan for nearby wireless networks");
        println!("  wifi connect <ssid> [pass]   - Connect and run 802.11 4-way handshake");
        println!("  wifi disconnect              - Disconnect from current network");
        println!("  wifi status                  - Display wireless link status");
        return;
    }

    match args[0] {
        "scan" => cmd_iwlist(&["scan"]),
        "connect" => {
            if args.len() < 2 {
                println!("Usage: wifi connect <ssid> [passphrase]");
                return;
            }
            let ssid = args[1];
            let pass = if args.len() > 2 { Some(args[2]) } else { None };

            println!("Connecting to WiFi network '{}'...", ssid);
            serial_println!("wifi: Connecting to WiFi network '{}'...", ssid);

            let mut wifi_lock = crate::net::wifi::WIFI_ADAPTER.lock();
            if let Some(ref mut adapter) = *wifi_lock {
                use crate::net::wifi::WiFiDriver;
                match adapter.connect(ssid, pass) {
                    Ok(()) => {
                        println!("Successfully associated and authenticated with '{}'!", ssid);
                        serial_println!("wifi: Successfully connected to '{}'", ssid);
                    }
                    Err(err) => {
                        println!("Failed to connect to '{}': {}", ssid, err);
                        serial_println!("wifi error: Failed to connect to '{}': {}", ssid, err);
                    }
                }
            } else {
                println!("wifi: No wireless adapter available.");
            }
        }
        "disconnect" => {
            let mut wifi_lock = crate::net::wifi::WIFI_ADAPTER.lock();
            if let Some(ref mut adapter) = *wifi_lock {
                use crate::net::wifi::WiFiDriver;
                match adapter.disconnect() {
                    Ok(()) => {
                        println!("wlan0: Disconnected from network.");
                        serial_println!("wifi: Disconnected from network.");
                    }
                    Err(err) => {
                        println!("wifi error: {}", err);
                    }
                }
            } else {
                println!("wifi: No wireless adapter available.");
            }
        }
        "status" => {
            let wifi_lock = crate::net::wifi::WIFI_ADAPTER.lock();
            if let Some(ref adapter) = *wifi_lock {
                use crate::net::wifi::WiFiDriver;
                let status = adapter.status();
                println!("Wireless Interface: {}", status.interface);
                println!("State:              {}", status.state);
                if let Some(ssid) = &status.ssid {
                    println!("SSID:               {}", ssid);
                }
                if let Some(bssid) = &status.bssid {
                    println!("BSSID:              {}", bssid);
                }
                if status.channel > 0 {
                    println!("Channel:            {} ({} MHz)", status.channel, status.frequency_mhz);
                    println!("Signal Level:       {} dBm (Quality: {}/{})", status.rssi, status.link_quality.0, status.link_quality.1);
                    println!("Bitrate:            {} Mb/s", status.bitrate_mbps);
                }
                println!("Packets:            Tx: {}, Rx: {}", status.tx_packets, status.rx_packets);
                println!("Bytes:              Tx: {}, Rx: {}", status.tx_bytes, status.rx_bytes);
            } else {
                println!("wifi: No wireless adapter available.");
            }
        }
        unknown => {
            println!("wifi: unknown subcommand '{}'. Use scan, connect, disconnect, or status.", unknown);
        }
    }
}

fn cmd_wpa_supplicant(args: &[&str]) {
    let mut ssid = None;
    let mut pass = None;
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-s" | "--ssid" => {
                if i + 1 < args.len() {
                    ssid = Some(args[i + 1]);
                    i += 1;
                }
            }
            "-p" | "--passphrase" | "-k" | "--key" => {
                if i + 1 < args.len() {
                    pass = Some(args[i + 1]);
                    i += 1;
                }
            }
            "-i" | "--interface" => {
                if i + 1 < args.len() {
                    i += 1;
                }
            }
            _ => {
                if ssid.is_none() {
                    ssid = Some(args[i]);
                } else if pass.is_none() {
                    pass = Some(args[i]);
                }
            }
        }
        i += 1;
    }

    if let Some(target_ssid) = ssid {
        let connect_args = if let Some(p) = pass {
            vec!["connect", target_ssid, p]
        } else {
            vec!["connect", target_ssid]
        };
        cmd_wifi(&connect_args);
    } else {
        println!("Usage: wpa_supplicant -s <ssid> [-p <passphrase>] [-i <interface>]");
    }
}

fn cmd_ping(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: ping <ipv4_address>");
        return;
    }
    let target_str = args[0];
    let target_ip = match parse_ipv4(target_str) {
        Some(ip) => ip,
        None => {
            println!("ping: invalid IPv4 address: '{}'", target_str);
            return;
        }
    };

    println!("PING {} 56(84) bytes of data.", target_ip);
    serial_println!("PING {} 56(84) bytes of data.", target_ip);

    let mut net_lock = crate::net::NETWORK.lock();
    if let Some(ref mut stack) = *net_lock {
        for seq in 1..=4 {
            match stack.ping(target_ip, 2000) {
                Ok(rtt) => {
                    println!("64 bytes from {}: icmp_seq={} time={} ms", target_ip, seq, rtt);
                    serial_println!("64 bytes from {}: icmp_seq={} time={} ms", target_ip, seq, rtt);
                }
                Err(err) => {
                    println!("From {}: icmp_seq={} Destination Host Unreachable ({})", target_ip, seq, err);
                    serial_println!("From {}: icmp_seq={} Destination Host Unreachable ({})", target_ip, seq, err);
                }
            }
        }
    } else {
        println!("ping: Network stack not active.");
        serial_println!("ping: Network stack not active.");
    }
}

fn cmd_curl(args: &[&str]) {
    if args.is_empty() {
        println!("Usage: curl <ipv4_address> [path] [port]");
        return;
    }
    let target_str = args[0];
    let target_ip = match parse_ipv4(target_str) {
        Some(ip) => ip,
        None => {
            println!("curl: invalid IPv4 address: '{}'", target_str);
            return;
        }
    };
    let path = if args.len() > 1 { args[1] } else { "/" };
    let port: u16 = if args.len() > 2 {
        args[2].parse().unwrap_or(80)
    } else {
        80
    };

    println!("Connecting to {}:{} for '{}'...", target_ip, port, path);
    serial_println!("curl: Connecting to {}:{} for '{}'...", target_ip, port, path);

    let mut net_lock = crate::net::NETWORK.lock();
    if let Some(ref mut stack) = *net_lock {
        match stack.http_get(target_ip, port, path, 5000) {
            Ok(body) => {
                println!("{}", body);
                serial_println!("{}", body);
            }
            Err(err) => {
                println!("curl error: {}", err);
                serial_println!("curl error: {}", err);
            }
        }
    } else {
        println!("curl: Network stack not active.");
        serial_println!("curl: Network stack not active.");
    }
}

fn cmd_lscpu() {
    let cpu = crate::cpu::get_cpu_info();
    println!("Architecture:                    x86_64");
    println!("CPU op-mode(s):                  32-bit, 64-bit");
    println!("Byte Order:                      Little Endian");
    println!(
        "Address sizes:                   {} bits physical, {} bits virtual",
        cpu.phys_addr_bits, cpu.virt_addr_bits
    );
    println!("CPU(s):                          {}", cpu.logical_cores);
    println!("Vendor ID:                       {}", cpu.vendor);
    println!("Model name:                      {}", cpu.brand);
    println!("CPU family:                      {}", cpu.family);
    println!("Model:                           {}", cpu.model);
    println!("Stepping:                        {}", cpu.stepping);
    let freq_mhz = BOOT_TIME_FREQ_MHZ.load(Ordering::Relaxed);
    println!("CPU MHz:                         {}", freq_mhz);

    let mut flags_line = String::new();
    for f in &cpu.features {
        flags_line.push_str(f);
        flags_line.push(' ');
    }
    println!("Flags:                           {}", flags_line.trim_end());

    serial_println!(
        "lscpu: Model='{}', Cores={}, Freq={} MHz",
        cpu.brand,
        cpu.logical_cores,
        freq_mhz
    );
}

fn cmd_acpi() {
    let acpi_lock = crate::acpi::ACPI_INFO.lock();
    if let Some(ref info) = *acpi_lock {
        println!("ACPI Root System Description Pointer (RSDP):");
        println!("  Status:             Found (Phys: {:#x})", info.rsdp_phys);
        println!(
            "  Revision:           {} (ACPI {}.0+)",
            info.revision,
            if info.revision >= 2 { "2" } else { "1" }
        );
        println!("  OEM ID:             {}", info.oem_id);
        println!("ACPI Fixed Description Table (FADT):");
        println!("  PM1a Control Block: {:#x}", info.pm1a_cnt_blk);
        println!(
            "  Hardware Reset:     {}",
            if info.reset_supported {
                "Supported"
            } else {
                "Not Available"
            }
        );
        if info.reset_supported {
            println!(
                "  Reset Register:     Port {:#x} (Value {:#x})",
                info.reset_port, info.reset_val
            );
        }
        println!("ACPI Multiple APIC Description Table (MADT):");
        println!("  Local APIC Base:    {:#x}", info.local_apic_addr);
        println!("  I/O APIC Base:      {:#x}", info.io_apic_addr);
        println!("  CPU Cores:          {}", info.num_cpu_cores);
        print!("  APIC Core IDs:      [");
        for (i, id) in info.cpu_apic_ids.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            print!("{}", id);
        }
        println!("]");
        serial_println!(
            "acpi: OEM='{}', Cores={}, LocalAPIC={:#x}",
            info.oem_id,
            info.num_cpu_cores,
            info.local_apic_addr
        );
    } else {
        println!("acpi: No ACPI tables detected.");
        serial_println!("acpi: No ACPI tables detected.");
    }
}

fn cmd_nvme(args: &[&str]) {
    let nvme_lock = crate::storage::nvme::NVME_CONTROLLER.lock();
    if let Some(ref dev) = *nvme_lock {
        let subcmd = args.first().copied().unwrap_or("status");
        match subcmd {
            "status" | "info" => {
                println!("{}", dev.info);
                serial_println!("{}", dev.info);
            }
            "identify" | "id-ctrl" => {
                let info = &dev.info;
                println!("NVMe Controller Identify Data:");
                println!("  Model Number (MN):       {}", info.model);
                println!("  Serial Number (SN):      {}", info.serial);
                println!("  Firmware Revision (FR):  {}", info.firmware);
                println!(
                    "  NVMe Version:            {}.{}",
                    info.version_major, info.version_minor
                );
                println!("  Total Sectors (LBA):     {}", info.total_sectors);
                println!("  Logical Block Size:      {} bytes", info.sector_size);
                let cap_gb = (info.total_sectors * info.sector_size as u64) / (1024 * 1024 * 1024);
                println!("  Total Capacity:          {} GB", cap_gb);
                println!(
                    "  Controller Type:         {}",
                    if info.is_hardware {
                        "PCIe Hardware Controller"
                    } else {
                        "NVMe PCIe Gen4 Controller (Laptop Profile)"
                    }
                );
                serial_println!("nvme identify: MN='{}', SN='{}'", info.model, info.serial);
            }
            _ => {
                println!("Usage: nvme <status|identify>");
            }
        }
    } else {
        println!("nvme: No NVMe controller registered.");
        serial_println!("nvme: No NVMe controller registered.");
    }
}

fn cmd_apt(args: &[&str]) {
    if args.is_empty() {
        println!("apt 2.6.0 (x86_64)");
        println!("Usage: apt <command> [options]");
        println!("  apt update             - Update list of available packages");
        println!("  apt list [--installed] - List available or installed packages");
        println!("  apt show <pkg>         - Show package details");
        println!("  apt install <pkg>      - Install packages with dependency resolution");
        println!("  apt remove <pkg>       - Remove installed packages");
        return;
    }

    let subcmd = args[0];
    let subargs = &args[1..];

    let mut apt_lock = crate::pkg::APT_MANAGER.lock();
    let apt = match apt_lock.as_mut() {
        Some(m) => m,
        None => {
            println!("apt: Package manager subsystem not initialized.");
            return;
        }
    };

    match subcmd {
        "update" => {
            println!("Hit:1 http://deb.microos.org/microos main InRelease");
            println!("Get:2 http://deb.microos.org/microos main/binary-x86_64 Packages [14.2 kB]");
            println!("Fetched 14.2 kB in 0.05s");
            println!("Reading package lists... Done");
            println!("Building dependency tree... Done");
            let count = apt.update();
            println!("{} packages can be installed.", count);
            serial_println!("apt update completed ({} packages).", count);
        }
        "list" => {
            let installed_only = subargs.iter().any(|&a| a == "--installed");
            let db_lock = crate::pkg::dpkg::DPKG_DB.lock();
            let db = db_lock.as_ref();

            println!("Listing... Done");
            for p in &apt.available {
                let is_inst = db.map(|d| d.is_installed(&p.name)).unwrap_or(false);
                if installed_only && !is_inst {
                    continue;
                }
                let status_badge = if is_inst { " [installed]" } else { "" };
                println!("{}/main {} {}{}", p.name, p.version, p.architecture, status_badge);
            }
        }
        "show" => {
            if subargs.is_empty() {
                println!("Usage: apt show <package_name>");
                return;
            }
            let pkg_name = subargs[0];
            if let Some(meta) = apt.show(pkg_name) {
                println!("Package: {}", meta.name);
                println!("Version: {}", meta.version);
                println!("Priority: {}", meta.priority);
                println!("Section: {}", meta.section);
                println!("Maintainer: {}", meta.maintainer);
                if !meta.depends.is_empty() {
                    let mut deps_str = String::new();
                    for (i, d) in meta.depends.iter().enumerate() {
                        if i > 0 {
                            deps_str.push_str(", ");
                        }
                        deps_str.push_str(d);
                    }
                    println!("Depends: {}", deps_str);
                }
                println!("Architecture: {}", meta.architecture);
                println!("Filename: {}", meta.filename);
                println!("Size: {} bytes", meta.size);
                println!("SHA256: {}", meta.sha256);
                println!("Description: {}", meta.description);
                serial_println!("apt show '{}'", pkg_name);
            } else {
                println!("N: Unable to locate package {}", pkg_name);
                serial_println!("apt show: package '{}' not found", pkg_name);
            }
        }
        "install" => {
            if subargs.is_empty() {
                println!("Usage: apt install <package_name>");
                return;
            }
            let pkg_name = subargs[0];
            println!("Reading package lists... Done");
            println!("Building dependency tree... Done");

            match apt.install(pkg_name) {
                Ok(installed) => {
                    for name in &installed {
                        println!("Unpacking {} ...", name);
                        println!("Setting up {} ...", name);
                    }
                    println!(
                        "0 upgraded, {} newly installed, 0 to remove.",
                        installed.len()
                    );
                    println!("Installation completed successfully.");
                    serial_println!("apt install '{}' succeeded: {:?}", pkg_name, installed);
                }
                Err(err) => {
                    println!("E: {}", err);
                    serial_println!("apt install error: {}", err);
                }
            }
        }
        "remove" => {
            if subargs.is_empty() {
                println!("Usage: apt remove <package_name>");
                return;
            }
            let pkg_name = subargs[0];
            println!("Reading package lists... Done");
            println!("Building dependency tree... Done");

            match apt.remove(pkg_name) {
                Ok(()) => {
                    println!("Removing {} ...", pkg_name);
                    println!("Purging configuration files for {} ...", pkg_name);
                    println!("Package '{}' removed successfully.", pkg_name);
                    serial_println!("apt remove '{}' succeeded.", pkg_name);
                }
                Err(err) => {
                    println!("E: {}", err);
                    serial_println!("apt remove error: {}", err);
                }
            }
        }
        unknown => {
            println!("E: Invalid operation '{}'", unknown);
        }
    }
}

fn cmd_dpkg(args: &[&str]) {
    if args.is_empty() {
        println!("Debian 'dpkg' package management program version 1.21.22 (x86_64).");
        println!("Usage: dpkg [options]");
        println!("  dpkg -l | --list           - List all installed packages");
        println!("  dpkg -s | --status <pkg>   - Display package status details");
        println!("  dpkg -L <pkg>              - List files installed by package");
        return;
    }

    let flag = args[0];
    let subargs = &args[1..];

    let db_lock = crate::pkg::dpkg::DPKG_DB.lock();
    let db = match db_lock.as_ref() {
        Some(d) => d,
        None => {
            println!("dpkg: Database not initialized.");
            return;
        }
    };

    match flag {
        "-l" | "--list" => {
            println!("Desired=Unknown/Install/Remove/Purge/Hold");
            println!(
                "| Status=Not/Inst/Conf-files/Unpacked/halF-conf/Half-inst/trig-aWait/Trig-pend"
            );
            println!("|/ Err?=(none)/Reinst-required (Status,Err: uppercase=bad)");
            println!("||/ Name           Version      Architecture Description");
            println!("+++-==============-============-============-=================================");
            for p in &db.installed {
                let desc_short = if p.description.len() > 33 {
                    &p.description[..33]
                } else {
                    &p.description
                };
                println!(
                    "ii  {:<14} {:<12} {:<12} {}",
                    p.name, p.version, p.architecture, desc_short
                );
            }
        }
        "-s" | "--status" => {
            if subargs.is_empty() {
                println!("Usage: dpkg -s <package_name>");
                return;
            }
            let pkg_name = subargs[0];
            if let Some(pkg) = db.find(pkg_name) {
                println!("Package: {}", pkg.name);
                println!("Status: {}", pkg.status);
                println!("Architecture: {}", pkg.architecture);
                println!("Version: {}", pkg.version);
                println!("Description: {}", pkg.description);
                if !pkg.files.is_empty() {
                    println!("Files:");
                    for f in &pkg.files {
                        println!(" {}", f);
                    }
                }
            } else {
                println!(
                    "dpkg-query: package '{}' is not installed and no info is available",
                    pkg_name
                );
            }
        }
        "-L" => {
            if subargs.is_empty() {
                println!("Usage: dpkg -L <package_name>");
                return;
            }
            let pkg_name = subargs[0];
            if let Some(pkg) = db.find(pkg_name) {
                for f in &pkg.files {
                    println!("{}", f);
                }
            } else {
                println!("dpkg-query: package '{}' is not installed", pkg_name);
            }
        }
        unknown => {
            println!("dpkg: unknown option '{}'", unknown);
        }
    }
}

fn cmd_reboot() -> ! {
    println!("Rebooting MicroOS...");
    serial_println!("Rebooting MicroOS via ACPI / 8042 reset...");
    crate::acpi::acpi_reboot();
}

fn cmd_shutdown() -> ! {
    println!("Shutting down MicroOS. Goodbye!");
    serial_println!("Shutting down MicroOS via ACPI power-off...");
    crate::acpi::acpi_poweroff();
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
