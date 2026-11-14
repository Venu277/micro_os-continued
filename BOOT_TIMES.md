# MicroOS — Boot Time Log

## Measurement Method

| Field | Description |
|-------|-------------|
| **Kernel time (ms)** | From `_start` to `init()` complete. Measured via `_rdtsc()`, calibrated dynamically against PIT Channel 2 (10 ms one-shot). Printed over serial as `BOOT_TIME_MS=<ms>.<us>`. |
| **QEMU total (s)** | VM power-on to serial "kernel ready" message. Measured with stopwatch, average of 3 runs. |
| **VMware total (s)** | Same, measured in VMware Workstation (available from Step 5 onward). |
| **Method** | Hardware timer calibration & measurement methodology. |

---

## Boot Time Table

| Step | Component Added | Kernel Init (ms) | QEMU Total (s) | VMware Total (s) | Method | Date |
|------|----------------|-----------------|----------------|-----------------|--------|------|
| Step 0 | Baseline (IDT only) | 43.972 ms | ~1.5 s | — | TSC ticks ÷ 2GHz est.; QEMU serial capture | 2026-11-14 |
| Step 1 | GDT/TSS, PIC remap, Timer & Keyboard IRQs | 41.896 ms | ~1.4 s | — | Dynamic PIT Ch2 calibrated TSC (@ 2505 MHz) | 2026-11-14 |

---

## Notes

- In Step 1, TSC calibration was upgraded from an arbitrary 2 GHz estimate to true hardware-calibrated frequency using PIT Channel 2 over a 10 ms interval (measured frequency: ~2,505 MHz on QEMU x86_64).
- Kernel initialization at 41.9 ms includes early VGA banner output, GDT loading, Task State Segment (TSS) loading, Interrupt Descriptor Table (IDT) configuration, 8259 PIC initialization, enabling interrupts, and hardware calibration.
- The entire QEMU cold boot from process spawn to kernel prompt takes ~1.4 seconds.
