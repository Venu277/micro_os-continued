# MicroOS — Progress Log

## Current Step: Step 3 — Tasks, Keyboard Input & Interactive Shell ✅

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
- Mapped 100 KiB heap at `0x_4444_4444_0000` with custom `LinkedListAllocator`.
- Enabled `alloc` crate for `Box`, `Vec`, and `String`. Verified with `tests/heap_allocation.rs`.

### Step 3: Tasks, Keyboard Input & Interactive Shell ✅
- **Async Task Runtime**: Implemented `Task` with unique `TaskId` and cooperative `Executor` supporting wakers and CPU power saving via `enable_and_hlt()`.
- **Async Keyboard Stream**: Implemented `ScancodeStream` with `next_scancode().await`, woken automatically on PS/2 keyboard IRQ1.
- **CMOS RTC & System Clock**: Implemented `src/rtc.rs` reading ports `0x70`/`0x71` with BCD decoding (`DateTime`), and calculated uptime from PIT timer ticks.
- **Interactive Shell**: Built command prompt (`micro_os> `) with line editing and Backspace support.
- **Built-in Commands**: `help`, `clear`, `echo`, `uptime`, `meminfo`, `bootime`, `date`, `reboot` (8042 reset), and `shutdown` (ACPI/QEMU power-off).
- **Boot Target Achieved**: Booted directly to interactive prompt; measured boot time: **54.341 ms**.

---

## Verification Evidence (Step 3)

```
cargo build  →  Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.72s (0 warnings)
cargo test   →  14 tests passing across 3 test targets:
                - src/lib.rs (8 tests: breakpoint, rtc_bcd, command_parsing, box, vec, trivial, println_many, println_simple) -> [ok]
                - tests/heap_allocation.rs (5 tests: simple, large_vec, many_boxes, long_lived, reuse) -> [ok]
                - tests/stack_overflow.rs (1 test: double fault IST switch) -> [ok]
cargo run    →  BOOT_TIME_MS=54.341 (TSC ticks=135412378, freq=2491 MHz)
                micro_os> help
                micro_os> meminfo -> total=102400B, used=840B, free=101560B
                micro_os> uptime  -> System Uptime: 0h 0m 3s (60 ticks)
                micro_os> date    -> CMOS RTC Clock: 2026-11-14 14:16:19 UTC
                micro_os> echo microos -> microos (with Backspace editing verified)
```

---

## Known Issues / Warnings

1. **Storage Is Ephemeral**: No disk driver or filesystem is mounted yet; files cannot be saved across reboots. This is the focus of Step 4.
2. **OneDrive File Sync**: Keep OneDrive sync paused when pushing to Git to prevent `.git/index.lock` contention.

---

## Decisions Made

| Decision | Chosen | Reason |
|----------|--------|--------|
| Task Scheduling | Cooperative Async/Await (`Future`) | Zero-cost abstractions, no thread stack overhead |
| Keyboard Delivery | `ScancodeStream` with Waker | Interrupt-driven, zero CPU spinning when idle |
| Clock Synchronization | PIT ticks + CMOS RTC | High-precision tick counting combined with real wall-clock date/time |
| Reboot Mechanism | 8042 Keyboard Reset (0x64 -> 0xFE) | Industry-standard BIOS/motherboard hardware reset pulse |
| Shutdown Mechanism | Multi-target ACPI / Port 0x604 | Compatible with QEMU, VMware, and VirtualBox |

---

## Next Step: Step 4 — Storage and a Proper File System

- Enumerate the PCI bus to detect ATA / IDE controllers.
- Implement ATA PIO block driver implementing a generic `BlockDevice` trait.
- Design a Virtual File System (VFS) abstraction.
- Implement a real on-disk filesystem (ext2-like: superblock, inode table, block bitmap, directory structure).
- Add persistence across reboot test.
- Add shell commands: `ls`, `cd`, `pwd`, `mkdir`, `cat`, `touch`, `write`, `rm`, `df`, `mkfs`.
