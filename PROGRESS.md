# MicroOS — Progress Log

## Current Step: Step 4 — Storage & On-Disk File System (MicroFS) ✅

**Date:** 2026-11-14

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

---

## Verification Evidence (Step 4)

```
cargo build  →  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.38s (0 warnings)
cargo test   →  22 tests passing across 4 test targets:
                - src/lib.rs (11 tests: microfs_structures, superblock_magic, breakpoint, rtc_bcd, command_parsing, storage_command_tokens, box, vec, trivial, println_many, println_simple) -> [ok]
                - tests/filesystem.rs (5 tests: format, create_and_read, directory_tree, large_file_indirect, reboot_persistence) -> [ok]
                - tests/heap_allocation.rs (5 tests: simple, large_vec, many_boxes, long_lived, reuse) -> [ok]
                - tests/stack_overflow.rs (1 test: double fault IST switch) -> [ok]
cargo run    →  ATA Primary Slave hard drive detected (8192 sectors, ~4 MiB)
                MicroFS superblock mounted successfully.
                BOOT_TIME_MS=90.428 (TSC ticks=226183661, freq=2501 MHz)
                micro_os:/> 
```

---

## Decisions Made

| Decision | Chosen | Reason |
|----------|--------|--------|
| Disk Interface | ATA PIO LBA28 | Universal compatibility across QEMU, VMware Workstation, and VirtualBox IDE controllers |
| Interrupt Handling | Polled PIO + nIEN=1 + IRQ 14/15 IDT stubs | Eliminates race conditions and prevents unhandled interrupt cascade |
| Filesystem Layout | MicroFS (ext2-inspired) | Clean layout: Superblock, bitmaps, inode table, directory entries, direct & single indirect blocks |
| Inode Size | Exactly 64 bytes with compile-time assertions | Prevents struct alignment issues; exactly 8 inodes per 512-byte sector |
| VFS Layer | Global `spin::Mutex<Vfs>` | Clean abstraction decoupling storage hardware from shell commands |

---

## Next Step: Step 5 — Packaging: Source Code to Bootable ISO, and VMware

- Script bootimage creation and hard disk attachment.
- Build bootable ISO using xorriso / El Torito hard disk emulation.
- Generate VMDK disk image via `qemu-img` for VMware Workstation.
- Test boot in QEMU with `-cdrom` and configure VMware Workstation VM.
