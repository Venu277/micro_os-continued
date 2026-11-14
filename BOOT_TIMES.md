# MicroOS — Boot Time Log

## Measurement Method

| Field | Description |
|-------|-------------|
| **Kernel time (ms)** | From `_start` to prompt / readiness. Measured via `_rdtsc()`, calibrated dynamically against PIT Channel 2 (10 ms one-shot). Printed over serial as `BOOT_TIME_MS=<ms>.<us>`. |
| **QEMU total (s)** | VM power-on to serial "kernel ready" message. Measured with stopwatch, average of 3 runs. |
| **VMware total (s)** | Same, measured in VMware Workstation (available from Step 5 onward). |
| **Method** | Hardware timer calibration & measurement methodology. |

---

## Boot Time Table

| Step | Component Added | Kernel Init (ms) | QEMU Total (s) | VMware Total (s) | Method | Date |
|------|----------------|-----------------|----------------|-----------------|--------|------|
| Step 0 | Baseline (IDT only) | 43.972 ms | ~1.5 s | — | TSC ticks ÷ 2GHz est.; QEMU serial capture | 2026-11-14 |
| Step 1 | GDT/TSS, PIC remap, Timer & Keyboard IRQs | 41.896 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2505 MHz) | 2026-11-14 |
| Step 2 | Paging, Frame Allocator, Kernel Heap, `alloc` | 50.801 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2484 MHz) | 2026-11-14 |

---

## Notes

- In Step 2, kernel initialization time includes full memory subsystem initialization: walking CR3, creating the recursive/offset page table, querying the BIOS memory map, mapping 25 pages of virtual memory for the 100 KiB heap, and executing early dynamic allocations (`Box`, `Vec`, `String`).
- Despite adding full virtual and physical memory management, kernel init completed in just 50.8 ms.
