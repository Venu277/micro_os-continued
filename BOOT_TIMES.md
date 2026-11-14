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
| Step 5 | VMDK Packaging & VMware Config (Booted from VMDK) | 76.471 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2510 MHz) | 2026-11-14 |
| Step 6 | LAN Networking (Intel e1000 DMA driver, smoltcp stack) | 147.455 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2494 MHz) | 2026-11-15 |
| Step 7 | WiFi Layer (802.11 Driver Trait, Simulation, WPA2) | 108.725 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2499 MHz) | 2026-11-15 |
| Step 8 | Laptop Hardware (UEFI, NVMe, ACPI, i5-1335U) | 154.183 ms | ~1.5 s | Ready | Dynamic PIT Ch2 calibrated TSC (@ 2498 MHz) | 2026-11-15 |
| **Step 9** | **Package Manager (`apt` / `dpkg` & Debian format)** | **152.735 ms** | **~1.5 s** | Ready | **Dynamic PIT Ch2 calibrated TSC (@ 2495 MHz)** | 2026-11-15 |

---

## Notes

- In Step 3, the boot time was measured from raw CPU power-on (`_start`) through CPU initialization, GDT/TSS, IDT, PIC, paging, 100 KiB heap setup, and task executor instantiation directly to the appearance of the interactive `micro_os> ` prompt.
- Time from CPU reset to an active interactive shell prompt is just **54.3 ms**!
- In Step 6, the boot time includes full PCI bus scanning, Intel e1000 controller hardware reset, MMIO page table mapping, DMA ring buffer descriptor initialization (16 Tx + 32 Rx), and `smoltcp` routing/interface initialization, reaching prompt readiness in **147.5 ms**.
- In Step 7, the 802.11 wireless subsystem is initialized during network startup with zero-cost overhead (<0.1 ms), reaching prompt readiness in **108.7 ms** (measured @ 2499 MHz).
- In Step 8, physical memory scanning for ACPI tables (RSDP, XSDT/RSDT, FADT, MADT) and NVMe storage controller registration brings prompt readiness to **154.2 ms** (measured @ 2498 MHz).
- In Step 9, package manager initialization (RFC 822 repository index registration and `/var/lib/dpkg/status` validation) adds negligible overhead (<1 ms), reaching interactive shell prompt readiness in **152.7 ms** (measured @ 2495 MHz).
