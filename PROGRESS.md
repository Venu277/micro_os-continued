# MicroOS — Progress Log

## Current Step: Step 2 — Memory Management ✅

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
- **Physical Memory Mapping**: Enabled bootloader's `map_physical_memory` feature; mapped complete physical memory space.
- **Type-Safe Entry Point**: Migrated from `_start` to `bootloader::entry_point!(kernel_main)` accepting `&'static BootInfo`.
- **OffsetPageTable & Paging**: Initialized active level 4 page table access at `Cr3::read()`.
- **BootInfoFrameAllocator**: Discovered free RAM regions from `boot_info.memory_map` and implemented `FrameAllocator<Size4KiB>`.
- **Kernel Heap & Linked-List Allocator**: Allocated and mapped 100 KiB virtual address space at `0x_4444_4444_0000`. Built custom `LinkedListAllocator` with in-memory free node tracking and memory reuse.
- **Enabled `alloc` Crate**: Added `"alloc"` to `build-std` in `.cargo/config.toml`, enabling `Box`, `Vec`, and `String`.
- **Integration Tests**: Added `tests/heap_allocation.rs` testing simple allocations, large vectors, many boxes, long-lived allocations, and memory reuse over 10,000 cycles.

---

## Verification Evidence (Step 2)

```
cargo build  →  Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.02s (0 warnings)
cargo test   →  12 tests total across 3 test targets:
                - src/lib.rs (6 tests: breakpoint, box, vec, trivial, println_many, println_simple) -> [ok]
                - tests/heap_allocation.rs (5 tests: simple, large_vec, many_boxes, long_lived, reuse) -> [ok]
                - tests/stack_overflow.rs (1 test: double fault IST switch) -> [ok]
cargo run    →  BOOT_TIME_MS=50.801 (TSC ticks=126221737, freq=2484 MHz)
                Heap test: Box = 42, Vec len = 10
                MicroOS Dynamic Heap Allocator Online!
                Kernel ready. Interrupts & Heap OK.
                Keyboard echo verified (typed "rust").
```

---

## Known Issues / Warnings

1. **Fixed Heap Size**: Heap size is currently fixed at 100 KiB. Dynamic heap extension can be added if workloads grow beyond 100 KiB.
2. **OneDrive File Sync**: Keep OneDrive sync paused when performing Git push commands to avoid temporary `.git/index.lock` contention.

---

## Decisions Made

| Decision | Chosen | Reason |
|----------|--------|--------|
| Entry Point | `bootloader::entry_point!` | Guaranteed type safety, stack alignment, and prevents UB |
| Paging | `OffsetPageTable` | Safe navigation of active level 4 tables via bootloader physical memory mapping |
| Physical Allocator | `BootInfoFrameAllocator` | Allocates exclusively from BIOS `Usable` memory regions |
| Heap Allocator | `LinkedListAllocator` (custom) | Self-contained, zero-dependency, reclaims memory on drop without heap fragmentation crashes |
| Heap Location | `0x_4444_4444_0000` (100 KiB) | Distinct high-memory address space, leaves low memory free |

---

## Next Step: Step 3 — Tasks, Keyboard Input & Interactive Shell

- Implement cooperative multitasking with an async/await executor (`Task`, `SimpleExecutor`, `Executor`).
- Implement async keyboard task reading scancodes from the lock-free queue.
- Implement an interactive shell with prompt (`micro_os> `) and line editing (backspace, enter).
- Shell commands: `help`, `clear`, `echo`, `uptime`, `meminfo`, `reboot`, `shutdown`, `bootime`.
- System clock combining PIT timer ticks and CMOS RTC for wall-clock time.
- Boot target: shell prompt ready.
