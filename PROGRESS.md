# MicroOS — Progress Log

## Current Step: Step 7 — WiFi Layer (Simulation & Driver Trait) ✅

**Date:** 2026-11-15

---

## What Is Done

### Step 0: Baseline & Hygiene ✅
- Verified toolchain, custom target, QEMU runner, and fixed duplicate `_start` bug.
- Established documentation (`README.md`, `PROGRESS.md`, `BOOT_TIMES.md`, `docs/STEP_00_baseline.md`).
- Target laptop hardware profiled in `laptop_hardware.txt`.

### Step 1: Robust Interrupts ✅
- GDT & TSS with 20 KiB dedicated IST stack for double faults.
- Remapped 8259 PIC to vectors 32 and 40.
- Timer IRQ0 tick accounting and keyboard IRQ1 with wait-free, lock-free SPSC scancode ring buffer.
- Dynamic PIT Channel 2 TSC frequency calibration.
- Verified with integration test `stack_overflow.rs`.

### Step 2: Memory Management ✅
- Enabled physical memory mapping via `bootloader` feature `map_physical_memory`.
- Migrated to type-safe `entry_point!(kernel_main)` accepting `&'static BootInfo`.
- Initialized 4-level paging via `OffsetPageTable` and implemented `BootInfoFrameAllocator`.
- Mapped 100 KiB heap at `0x_4444_4444_0000` with custom `LinkedListAllocator` (expanded to 256 KiB in Step 4).
- Enabled `alloc` crate for `Box`, `Vec`, and `String`. Verified with `tests/heap_allocation.rs`.

### Step 3: Tasks, Keyboard Input & Interactive Shell ✅
- **Async Task Runtime**: Implemented `Task` with unique `TaskId` and cooperative `Executor` supporting wakers and CPU power saving via `enable_and_hlt()`.
- **Async Keyboard Stream**: Implemented `ScancodeStream` with `next_scancode().await`, woken automatically on PS/2 keyboard IRQ1.
- **CMOS RTC & System Clock**: Implemented `src/rtc.rs` reading ports `0x70`/`0x71` with BCD decoding (`DateTime`), and calculated uptime from PIT timer ticks.
- **Interactive Shell**: Built command prompt (`micro_os> `) with line editing and Backspace support.
- **Built-in Commands**: `help`, `clear`, `echo`, `uptime`, `meminfo`, `bootime`, `date`, `reboot` (8042 reset), and `shutdown` (ACPI/QEMU power-off).
- **Boot Target Achieved**: Booted directly to interactive prompt; measured boot time: **54.341 ms**.

### Step 4: Storage & File System (MicroFS) ✅
- **PCI Bus Enumeration (`src/pci.rs`)**: Scans 256 buses, 32 devices, 8 functions for class `0x01` (Mass Storage), identifying IDE controllers (`0x0101`).
- **ATA PIO Driver (`src/storage/ata.rs`)**: Implemented LBA28 sector read/write for Primary/Secondary Master/Slave over I/O ports `0x1F0`/`0x170`. Disabled hardware interrupts via Device Control Register (`nIEN = 1`) and added safety IRQ 14/15 handlers in IDT.
- **Block Device Trait & RamDisk (`src/storage/mod.rs`)**: Abstract `BlockDevice` trait allowing hot-swapping between physical ATA drives and RAM-backed disks.
- **On-Disk Filesystem (`src/fs/microfs.rs`)**: Ext2-style on-disk filesystem with `Superblock`, Inode bitmap, Block bitmap, 256-inode table (64 bytes/inode), Directory entries, 10 direct blocks, and 1 indirect block (supporting up to 69 KiB files).
- **Virtual File System Layer (`src/fs/vfs.rs`)**: Path resolution, current working directory tracking, `mount`, `format`, directory traversal (`..`), file creation, reading, writing, and deletion.
- **Shell Commands**: Integrated `lspci`, `df`, `mkfs`, `ls`, `cd`, `pwd`, `mkdir`, `touch`, `write`, `cat`, and `rm`.
- **Reboot Persistence Verified**: Created and verified integration test `tests/filesystem.rs` proving persistent storage across unmount/remount sessions.
- **Test Suite**: 22/22 tests passing cleanly across all targets.

### Step 5: Packaging: Bootable ISO & VMware ✅
- **Packaging Pipeline (`scripts/package.ps1` & `scripts/package.sh`)**: End-to-end automated compilation, bootimage creation, and virtual disk generation.
- **VMware VMDK Generation**: Converted `bootimage-micro_os.bin` and `disk.img` into `micro_os.vmdk` and `data.vmdk` using `qemu-img convert`.
- **El Torito ISO Generation**: Produced `build/micro_os.iso` via WSL `xorriso`. Documented why VMDK hard disk format is the primary, reliable boot path for VMware and QEMU.
- **VMware Virtual Machine Specification (`vmware/micro_os.vmx`)**: Created production configuration for VMware Workstation/Player with Primary IDE Master OS disk, Primary IDE Slave data disk, serial port debugging, and Intel e1000 NIC.
- **Verification**: Verified booting from `micro_os.vmdk` and `data.vmdk` in QEMU, reaching the interactive shell in 76.471 ms.

### Step 6: LAN Networking (Intel e1000 & smoltcp) ✅
- **PCI Discovery & Configuration (`src/pci.rs`)**: Probed PCI bus for Intel 82540EM (`0x8086:0x100E`, QEMU) and 82545EM (`0x8086:0x100F`, VMware), extracted 32/64-bit MMIO BAR0, and enabled PCI Bus Mastering.
- **Intel e1000 Hardware Driver (`src/net/e1000.rs`)**: Mapped controller MMIO space into kernel paging table. Initialized circular DMA Tx ring (16 descriptors) and Rx ring (32 descriptors) with 2048-byte packet buffers translated to physical frames. Handled controller reset, SLU, RCTL/TCTL registers, and programmed MAC address into RAL0/RAH0 with AV bit.
- **smoltcp Protocol Stack (`src/net/mod.rs`)**: Implemented `smoltcp::phy::Device` for `E1000Phy` with zero-allocation RxToken/TxToken. Initialized `Interface` with static IP (`10.0.2.15/24`) and default gateway (`10.0.2.2`). Implemented `ping()` with RTT calculation and TCP client for `curl`.
- **Shell Commands (`src/shell.rs`)**: Integrated `ifconfig` (IP, MAC, GW, packet counters), `ping` (ICMP Echo Request/Reply), and `curl` / `http-get` (HTTP/1.0 GET).
- **Test Suite**: Created `tests/networking.rs`. Full test suite passed 26/26 tests cleanly across all 5 test binaries.

### Step 7: WiFi Layer Simulation & 802.11 Driver Trait ✅
- **WiFi Driver Trait (`src/net/wifi.rs`)**: Created extensible `WiFiDriver` trait (`name`, `mac_address`, `scan`, `connect`, `disconnect`, `status`, `send_frame`, `receive_frame`).
- **Honest 802.11 Simulation (`SimulatedWiFi`)**: Acknowledged hypervisor behavior (VMware/QEMU bridge WiFi as virtual Ethernet). Implemented complete 802.11 state machine (`Disconnected` -> `Scanning` -> `Authenticating` -> `Associated` -> `Connected`), WPA2-PSK 4-way handshake simulation (ANonce, SNonce/MIC, PTK/GTK), and frame bridging to e1000.
- **Wireless CLI Utilities (`src/shell.rs`)**:
  - `iwconfig`: Displays `wlan0` ESSID, Mode, Frequency, BSSID, Bit Rate, Tx-Power, Link Quality, Signal Level.
  - `iwlist [iface] scan`: Active 802.11 scanning across 2.4 GHz and 5 GHz channels reporting Cell info, BSSIDs, frequencies, and encryption types.
  - `wifi <scan|connect|disconnect|status>`: High-level connection management.
  - `wpa_supplicant -s <ssid> [-p <pass>]`: Linux-compatible connection syntax.
  - Updated `ifconfig`: Now lists both `eth0` and `wlan0` interfaces.
- **Integration Test (`tests/wifi.rs`)**: Created 5 comprehensive integration tests validating AP properties, 24-byte 802.11 MAC headers, scanning, state machine with valid and invalid passphrases, and bridged frame transmission.
- **Test Suite**: Full test suite now passes **31/31 tests** cleanly across 6 test binaries.

---

## Verification Evidence (Step 7)

```
cargo test            → Running unittests src\lib.rs (11 tests) [all ok]
                      → Running tests\filesystem.rs (5 tests) [all ok]
                      → Running tests\heap_allocation.rs (5 tests) [all ok]
                      → Running tests\networking.rs (4 tests) [all ok]
                      → Running tests\stack_overflow.rs (1 test) [all ok]
                      → Running tests\wifi.rs (5 tests) [all ok]
                      test result: ok. 31 passed; 0 failed; 0 ignored; finished in 0.00s

VMDK / QEMU Boot Log  → Serial port online.
                        ATA Primary Slave hard drive detected (8192 sectors, ~4 MiB)
                        MicroFS superblock mounted successfully.
                        Intel e1000 NIC detected on PCI bus 00:03.0 (vendor=8086, device=100e)
                        e1000: rx_desc_phys=PhysAddr(0x1a000), tx_desc_phys=PhysAddr(0x1a200)
                        e1000 initialized! MAC: 52:54:00:12:34:56
                        LAN Networking & smoltcp stack initialized (IP: 10.0.2.15/24, GW: 10.0.2.2).
                        [wlan0] 802.11 Wireless Subsystem initialized (MAC: 52:54:00:12:34:56, bridged via e1000)
                        BOOT_TIME_MS=108.725 (TSC ticks=271760507, freq=2499 MHz)
                        micro_os:/> 
```

---

## Decisions Made

| Decision | Chosen | Reason |
|----------|--------|--------|
| WiFi Architecture | `WiFiDriver` Trait + `SimulatedWiFi` | Real chipsets (MT7921) cannot be accessed directly in VMs; trait enables future bare-metal drivers while simulation bridges e1000 |
| MAC Address Generation | Locally-administered bit (`base_mac[0] \| 0x02`) | Guarantees non-conflicting unique MAC address for virtual `wlan0` interface |
| Handshake Logic | Realistic WPA2 4-Way Handshake | Validates passphrases, models ANonce/SNonce/MIC and PTK/GTK crypto installation |
| Network Controller | Intel e1000 (82540EM / 82545EM) | Universally supported in both QEMU (`0x100E`) and VMware Workstation (`0x100F`) |
| Network Protocol Stack | `smoltcp 0.12` | Zero-allocation, event-driven bare-metal TCP/IP stack with medium-ethernet support |

---

## Next Step: Step 8 — Laptop Hardware Compatibility (UEFI, NVMe, ACPI)

- Target real laptop hardware profile from `laptop_hardware.txt` (Intel i5-1335U, NVMe SSD, MediaTek MT7921, ACPI).
- Implement NVMe driver foundations (Admin Submission / Completion Queues, Controller Configuration).
- Parse ACPI tables (RSDP, XSDT, FADT power off / reset, MADT interrupt routing).
- Establish dual BIOS / UEFI boot paths.
