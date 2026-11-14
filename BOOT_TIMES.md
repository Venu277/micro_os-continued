# MicroOS — Boot Time Log

## Measurement Method

| Field | Description |
|-------|-------------|
| **Kernel time (ms)** | From `_start` to prompt / readiness. Measured via `_rdtsc()`, calibrated dynamically against PIT Channel 2 (10 ms one-shot). Printed over serial as `BOOT_TIME_MS=<ms>.<us>`. |
| **QEMU total (s)** | VM power-on to serial "kernel ready" / prompt message. Measured with stopwatch, average of 3 runs. |
| **VMware total (s)** | Same, measured in VMware Workstation (available from Step 5 onward). |
| **Method** | Hardware timer calibration & measurement methodology. |

---

## Boot Time Table

| Step | Component Added | Kernel Init (ms) | QEMU Total (s) | VMware Total (s) | Method | Date |
|------|----------------|-----------------|----------------|-----------------|--------|------|
| Step 0 | Baseline (IDT only) | 43.972 ms | ~1.5 s | — | TSC ticks ÷ 2GHz est.; QEMU serial capture | 2026-11-14 |
| Step 1 | GDT/TSS, PIC remap, Timer & Keyboard IRQs | 41.896 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2505 MHz) | 2026-11-14 |
| Step 2 | Paging, Frame Allocator, Kernel Heap, `alloc` | 50.801 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2484 MHz) | 2026-11-14 |
| Step 3 | Async Executor, Shell Prompt, CMOS RTC, Clock | 54.341 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2491 MHz) | 2026-11-14 |
| Step 4 | Storage, ATA PIO Driver, MicroFS on-disk filesystem, VFS | 90.428 ms | ~1.5 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2501 MHz) | 2026-11-14 |
| **Step 5** | **VMDK Packaging & VMware Config (Booted from VMDK)** | **76.471 ms** | **~1.5 s** | Ready | **Dynamic PIT Ch2 calibrated TSC (@ 2510 MHz)** | 2026-11-14 |

---

## Notes

- In Step 3, the boot time was measured from raw CPU power-on (`_start`) through CPU initialization, GDT/TSS, IDT, PIC, paging, 100 KiB heap setup, and task executor instantiation directly to the appearance of the interactive `micro_os> ` prompt.
- Time from CPU reset to an active interactive shell prompt is just **54.3 ms**!
