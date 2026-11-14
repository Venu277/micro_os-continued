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
| Step 3 | Async executor, shell, system clock | 🔲 Next |
| Step 4 | Storage, block driver, file system | 🔲 Planned |
| Step 5 | ISO packaging, VMware boot | 🔲 Planned |
| Step 6 | LAN networking (e1000, smoltcp) | 🔲 Planned |
| Step 7 | WiFi layer (honest simulation & driver trait) | 🔲 Planned |
| Step 8 | Laptop hardware compatibility (UEFI, NVMe, ACPI) | 🔲 Planned |
| Step 9 | Package manager `apt` / `aptitude` | 🔲 Planned |
| Step 10 | Package store `snap` | 🔲 Planned |
| Step 11 | Final integration and presentation | 🔲 Planned |

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
│   ├── lib.rs           # Library root: test framework, init, hlt_loop
│   ├── main.rs          # Kernel entry point (entry_point! macro)
│   ├── gdt.rs           # GDT and TSS with IST double-fault stack
│   ├── interrupts.rs    # IDT, PIC remap, timer, keyboard ISR, lock-free queue, PIT calibration
│   ├── memory.rs        # Paging via OffsetPageTable and BootInfoFrameAllocator
│   ├── allocator.rs     # Kernel heap initialization and global allocator registration
│   │   └── linked_list.rs # Linked-list allocator with memory reclamation
│   ├── vga_buffer.rs    # VGA text-mode driver and println! macro
│   └── serial.rs        # UART serial port driver and serial_println! macro
├── tests/
│   ├── heap_allocation.rs # Integration test: Box, dynamic Vec, multiple allocations, reuse
│   └── stack_overflow.rs  # Integration test: double-fault stack switching via IST
├── docs/                # Per-step documentation
│   ├── STEP_00_baseline.md
│   ├── STEP_01_robust_interrupts.md
│   └── STEP_02_memory_management.md
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

cargo build          # Compile the kernel with alloc enabled
cargo test           # Run all 12 tests across unit and integration suites
cargo run            # Boot in QEMU (serial output visible in terminal)
```

---

## License

MIT
