# MicroOS — Progress Log

## Current Step: Step 1 — Robust Interrupts ✅

**Date:** 2026-11-14

---

## What Is Done

### Pre-existing (verified in Step 0)
- Rust kernel, x86_64, BIOS boot via `bootloader = "0.9"` + `bootimage 0.10.3`
- Pinned toolchain: `nightly-2025-06-01` (rust-src, llvm-tools-preview)
- Custom target `x86_64-micro_os.json`: 64-bit, softfloat, lld linker, panic=abort
- bootloader 0.9.35 target JSON hand-patched (pointer-width strings + rustc-abi)
- VGA `println!` macro, serial `serial_println!` macro
- Library/binary split: `src/lib.rs` + `src/main.rs`
- Custom test framework with `isa-debug-exit` (exit code 33 = success)

### Added in Step 0
- Fixed duplicate `#[cfg(test)] _start` in `src/lib.rs`
- Base documentation: `README.md`, `PROGRESS.md`, `BOOT_TIMES.md`, `docs/STEP_00_baseline.md`
- Laptop hardware specifications stored in `laptop_hardware.txt`

### Added in Step 1
- **GDT & TSS with IST**: `src/gdt.rs` sets up Global Descriptor Table with Kernel Code Segment and Task State Segment. Allocates a dedicated 20 KiB stack at IST index 0 for double faults.
- **PIC 8259 Remapping**: `src/interrupts.rs` remaps master PIC to vector 32 and slave PIC to vector 40.
- **Timer & Keyboard Handlers**: Handled IRQ0 (vector 32) for timer tick accounting and IRQ1 (vector 33) for keyboard.
- **Lock-Free SPSC Scancode Queue**: Keyboard ISR pushes scancodes into an atomic ring buffer without locking, eliminating interrupt-context deadlocks. Scancodes are decoded in the main task loop.
- **PIT-Based TSC Calibration**: Uses PIT Channel 2 one-shot measurement over 10 ms to accurately calibrate CPU TSC clock frequency (~2505 MHz in QEMU).
- **Stack Overflow Integration Test**: `tests/stack_overflow.rs` proves that double fault catches stack overflow via IST stack without triple-faulting.
- **Documentation**: `docs/STEP_01_robust_interrupts.md` with detailed architecture and diagrams.

---

## Verification Evidence (Step 1)

```
cargo build  →  Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.33s (0 warnings)
cargo test   →  Running 4 unit tests
                micro_os::interrupts::test_breakpoint_exception...  [ok]
                micro_os::trivial_assertion...                       [ok]
                micro_os::vga_buffer::test_println_many...           [ok]
                micro_os::vga_buffer::test_println_simple...         [ok]
                Running integration test
                stack_overflow::stack_overflow...                    [ok]
cargo run    →  BOOT_TIME_MS=41.896 (TSC ticks=104952366, freq=2505 MHz)
                Kernel ready. Interrupts OK.
                Keyboard interactive typing echo verified via QEMU monitor.
```

---

## Known Issues / Warnings

1. **Memory Allocations Still Static**: Step 2 will implement dynamic memory management (`alloc`, `Box`, `Vec`, heap).
2. **OneDrive File Sync**: Keep OneDrive sync paused when running Git commands to avoid temporary lock contention.

---

## Decisions Made

| Decision | Chosen | Reason |
|----------|--------|--------|
| Double-fault Stack | 20 KiB dedicated IST stack | Prevents triple faults on kernel stack overflow |
| PIC Vectors | 32 (PIC1) and 40 (PIC2) | Avoids CPU exception range 0–31 |
| Scancode Handling | Lock-free SPSC Atomic Queue | Prevents mutex deadlocks in interrupt handlers |
| TSC Calibration | PIT Channel 2 (10 ms) | Provides real-world millisecond timing without external dependencies |

---

## Next Step: Step 2 — Memory Management

- Enable bootloader's `map_physical_memory` feature.
- Change entry point to `entry_point!(kernel_main)` using `bootloader::BootInfo`.
- Implement frame allocator (`BootInfoFrameAllocator`) from bootloader memory map.
- Map virtual heap pages to physical frames.
- Implement an allocator (linked-list or fixed-size-block) and enable Rust `alloc` crate.
- Tests: heap allocations, `Box`, `Vec`, reallocation, and memory reuse.
