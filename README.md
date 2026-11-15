# MicroOS

A hobby x86_64 operating system kernel written in Rust, built from scratch as a
learning project. It boots in QEMU and is being developed step-by-step
toward a VMware-bootable OS with networking, storage, and a package manager.

> **Honest Scope:** See the [Honest Scope](#honest-scope) section below.

---

## Current Status

| Step | Name | Status |
|------|------|--------|
| Step 0 | Baseline, hygiene, TSC boot timer | ✅ Done |
| Step 1 | GDT/TSS, PIC remap, timer & keyboard IRQs, IST | ✅ Done |
| Step 2 | Paging, frame allocator, heap, `alloc` | ✅ Done |
| Step 3 | Async executor, shell, system clock | ✅ Done |
| Step 4 | Storage, block driver, file system | ✅ Done |
| Step 5 | ISO packaging, VMware boot | ✅ Done |
| Step 6 | LAN networking (e1000, smoltcp) | ✅ Done |
| Step 7 | WiFi layer (honest simulation & driver trait) | ✅ Done |
| Step 8 | Laptop hardware compatibility (UEFI, NVMe, ACPI) | ✅ Done |
| Step 9 | Package manager `apt` / `aptitude` | ✅ Done |
| Step 10 | Package store `snap` | ✅ Done |
| Step 11 | Final integration and presentation | ✅ Done |

See [PROGRESS.md](PROGRESS.md) for detailed milestone notes and [BOOT_TIMES.md](BOOT_TIMES.md) for timing progression.

---

## Honest Scope

This project builds **MicroOS-native** tools that share command names and formats
with Linux, but are **not** Linux-compatible:

- **`apt` / `aptitude`**: Parses real Debian `Packages` index files and `.deb`
  archive format (ar + control.tar + data.tar), implements dependency resolution
  and a dpkg-style status database — but packages must be built for MicroOS.
  Prebuilt Debian/Ubuntu `.deb` binaries will **not** run; they require Linux
  kernel ABI, glibc, and ELF dynamic linking against the Linux dynamic linker.

- **`snap` store**: A snap-style package store with its own manifest format and
  HTTP index. This is **not** real snapd/squashfs. Real Snap packages require
  snapd, squashfs-tools, AppArmor, and the Linux kernel.

- **WiFi in VMware**: The host's WiFi NIC is presented as an Ethernet device to
  the guest. A real chipset driver (e.g. Intel/MediaTek) cannot be exercised in a
  VM. The WiFi layer is implemented with a clean driver trait so a real driver
  can be added when running on bare metal.

- **No Linux ABI**: MicroOS has its own syscall interface. Linux ELF binaries will
  not run.

---

## Project Structure

```
micro_os-main/
├── .cargo/              # Cargo build config (custom target, build-std = core, compiler_builtins, alloc)
├── src/
│   ├── lib.rs           # Library root: test framework, init, init_storage, hlt_loop
│   ├── main.rs          # Kernel entry point (entry_point! macro)
│   ├── cpu.rs           # CPUID architecture & feature enumeration (Intel Core i5-1335U profile)
│   ├── acpi.rs          # ACPI 2.0+ parser (RSDP, XSDT, FADT power/reset, MADT APIC cores)
│   ├── gdt.rs           # GDT and TSS with IST double-fault stack
│   ├── interrupts.rs    # IDT, PIC remap, timer, keyboard ISR, lock-free queue, PIT calibration, ATA IRQ stubs
│   ├── memory.rs        # Paging via OffsetPageTable and BootInfoFrameAllocator
│   ├── allocator.rs     # Kernel heap initialization (256 KiB) and global allocator registration
│   │   └── linked_list.rs # Linked-list allocator with memory reclamation
│   ├── pci.rs           # PCI bus enumeration and device identification
│   ├── net/             # Networking subsystem
│   │   ├── mod.rs       # NetworkStack, smoltcp::phy::Device, ping, curl/http_get
│   │   ├── e1000.rs     # Intel 82540EM/82545EM driver with circular DMA descriptor rings
│   │   └── wifi.rs      # WiFiDriver trait, SimulatedWiFi, 802.11 frames, WPA2 state machine
│   ├── storage/         # Storage driver subsystem
│   │   ├── mod.rs       # BlockDevice trait and RamDisk
│   │   ├── ata.rs       # ATA PIO LBA28 hard disk controller driver
│   │   └── nvme.rs      # NVMe 1.4+ controller architecture (64B SQE, 16B CQE, 512GB SSD)
│   ├── fs/              # Filesystem subsystem
│   │   ├── mod.rs       # Module re-exports
│   │   ├── microfs.rs   # Ext2-style on-disk filesystem (Superblock, Bitmaps, Inodes, DirEntries)
│   │   └── vfs.rs       # Virtual File System layer (mount, path resolution, cwd, file operations)
│   ├── pkg/             # Package management subsystem
│   │   ├── mod.rs       # AptManager interface and preloaded repository definitions
│   │   ├── deb.rs       # .deb container parser/builder (ar archive + USTAR tar reader)
│   │   ├── deps.rs      # Topological dependency graph resolver (Kahn's algorithm)
│   │   ├── dpkg.rs      # Dpkg status DB (/var/lib/dpkg/status) and VFS unpacker
│   │   └── rfc822.rs    # RFC 822 / Debian 822 Packages & control parser
│   ├── snap/            # Snap container package store subsystem
│   │   ├── mod.rs       # SnapManager, store catalog, and sandbox execution runner
│   │   ├── manifest.rs  # meta/snap.yaml parser, confinement (strict/classic), plugs/slots
│   │   ├── container.rs # SNAP01 binary package container reader and builder
│   │   └── environment.rs # Loopback mounts (/snap/<name>/<rev>), current links, wrappers
│   ├── rtc.rs           # CMOS RTC real-time clock and PIT system uptime
│   ├── shell.rs         # Interactive CLI shell (apt, dpkg, snap, storage, LAN, WiFi, hardware: lscpu, acpi, nvme)
│   ├── task/            # Cooperative asynchronous multitasking
│   │   ├── mod.rs       # Task and TaskId abstractions
│   │   ├── executor.rs  # Async Executor with idle sleep (enable_and_hlt)
│   │   └── keyboard.rs  # ScancodeStream with hardware waker bridge
│   ├── vga_buffer.rs    # VGA text-mode driver with clear_screen and backspace
│   └── serial.rs        # UART serial port driver and serial_println! macro
├── scripts/             # Automated packaging and boot scripts
│   ├── package.ps1      # PowerShell packaging pipeline (bootimage -> VMDK + ISO)
│   ├── package.sh       # Bash / Linux / WSL packaging pipeline
│   └── run_vmdk.ps1     # Launcher to boot packaged VMDKs in QEMU
├── vmware/              # VMware Workstation configuration
│   ├── micro_os.vmx     # Virtual machine configuration (IDE disks, serial, e1000)
│   └── README.md        # Step-by-step instructions for VMware Workstation
├── tests/
│   ├── filesystem.rs      # Integration test: format, create/read, dir trees, large indirect files, reboot persistence
│   ├── hardware.rs        # Integration test: CPUID brand/features, ACPI checksums & tables, NVMe queues
│   ├── heap_allocation.rs # Integration test: Box, dynamic Vec, multiple allocations, reuse
│   ├── networking.rs      # Integration test: IPv4 parser, DMA descriptors, ICMP formatting, gateway ping
│   ├── pkg_apt.rs         # Integration test: RFC 822 parser, .deb ar/tar unpacker, dependency resolver, dpkg status
│   ├── smoke_integration.rs # Integration test: end-to-end smoke test across all subsystems
│   ├── snap_store.rs      # Integration test: snap.yaml parser, SNAP01 containers, mount layout, rollback
│   ├── stack_overflow.rs  # Integration test: double-fault stack switching via IST
│   └── wifi.rs            # Integration test: AP properties, 802.11 MAC headers, scanning, WPA2 handshake
├── docs/                # Per-step documentation
│   ├── STEP_00_baseline.md
│   ├── STEP_01_robust_interrupts.md
│   ├── STEP_02_memory_management.md
│   ├── STEP_03_tasks_and_shell.md
│   ├── STEP_04_storage_and_filesystem.md
│   ├── STEP_05_iso_and_vmware.md
│   ├── STEP_06_lan_networking.md
│   ├── STEP_07_wifi_layer.md
│   ├── STEP_08_laptop_hardware.md
│   ├── STEP_09_package_manager_apt.md
│   ├── STEP_10_snap_store.md
│   └── STEP_11_final_integration_and_presentation.md
├── Cargo.toml           # Package manifest
├── rust-toolchain.toml  # Pins nightly-2025-06-01
├── x86_64-micro_os.json # Custom target spec (no_std, softfloat, ld.lld)
├── PROGRESS.md          # Current state, known issues, decisions
├── BOOT_TIMES.md        # Boot time measurements per step
├── laptop_hardware.txt  # Target laptop hardware profile for Step 8
└── README.md            # This file
```

---

## Prerequisites

| Tool | Version / Notes |
|------|----------------|
| Rust (rustup) | Toolchain pinned to `nightly-2025-06-01` via `rust-toolchain.toml` |
| `rust-src` component | Required to build `core` and `alloc` for the custom target |
| `llvm-tools-preview` | Required by `bootimage` to objcopy the kernel binary |
| `bootimage` | 0.10.3 — install with `cargo install bootimage --version 0.10.3` |
| QEMU | Any recent x86_64 build — on Windows add to PATH: `C:\Program Files\qemu` |

---

## Build and Run

```powershell
# From the project root on Windows PowerShell:
$env:PATH += ";C:\Program Files\qemu"

cargo build          # Compile the kernel
cargo test           # Run all 36 unit and integration tests
cargo run            # Boot into interactive shell in QEMU
```

---

## License

MIT
