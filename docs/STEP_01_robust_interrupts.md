# Step 01: Robust Interrupts (GDT, TSS, PIC Remap, Timer & Keyboard IRQs)

## 1. Goal of the Step
Upgrade MicroOS from basic exception handling to a robust, hardware-backed interrupt subsystem:
- Implement a **Global Descriptor Table (GDT)** and **Task State Segment (TSS)** with an **Interrupt Stack Table (IST)** dedicated to double faults.
- Remap the legacy **8259 PIC** to avoid collisions with x86 CPU exception vectors (0–31).
- Handle the **PIT Timer (IRQ0)** and **PS/2 Keyboard (IRQ1)** with End-of-Interrupt (EOI) signaling.
- Decouple keyboard interrupt handling using a wait-free, lock-free single-producer single-consumer (**SPSC**) scancode queue to prevent interrupt deadlocks.
- Accurately calibrate the CPU **Time Stamp Counter (TSC)** frequency using PIT Channel 2.
- Verify double fault handling using an automated **stack overflow integration test**.

---

## 2. What Was Added / Changed

| File | Purpose |
|---|---|
| `src/gdt.rs` | Defines GDT, TSS, and allocates a dedicated 20 KiB stack in IST index 0 for double faults. |
| `src/interrupts.rs` | Remaps 8259 PIC (32/40), registers IST double-fault handler, implements Timer (IRQ0) and Keyboard (IRQ1) ISRs, lock-free `ScancodeQueue`, and PIT TSC calibration. |
| `src/lib.rs` | Exports `gdt` module, updates `init()` to load GDT, IDT, initialize PICs, and enable interrupts (`sti`). |
| `src/main.rs` | Calibrates TSC using PIT, prints calibrated boot time, and runs non-blocking keyboard echo loop outside interrupt context. |
| `Cargo.toml` | Declares `[[test]] name = "stack_overflow" harness = false` for the double fault integration test. |
| `tests/stack_overflow.rs` | Integration test triggering infinite recursion stack overflow to verify IST stack switching on double fault. |
| `BOOT_TIMES.md` | Recorded calibrated boot time entry for Step 1. |
| `PROGRESS.md` | Updated current milestone, completed features, decisions, and known status. |
| `README.md` | Updated feature matrix and interrupt architecture notes. |

---

## 3. How It Works

### Plain English Explanation
When a computer runs, two kinds of interruptions can happen:
1. **Internal CPU Exceptions**: Caused directly by code (e.g., dividing by zero, accessing an unmapped memory address, or a stack running out of space).
2. **External Hardware Interrupts**: Sent by physical devices (e.g., a hardware clock ticking or a user pressing a key on the keyboard).

Before this step, hardware interrupts were mapped by default to CPU exception vectors 0–15, meaning a timer tick looked like a double fault to the CPU! We remapped the Programmable Interrupt Controller (PIC) to vectors 32–47.

Additionally, if the kernel stack overflows, the CPU cannot push exception information onto that same stack—doing so would trigger a second fault (a double fault), which also fails to push onto the broken stack, triggering a fatal **triple fault** that instantly resets the PC. We introduced a **Task State Segment (TSS)** with a separate **Interrupt Stack Table (IST)**. When a double fault occurs, the CPU automatically switches to this clean, dedicated emergency stack, preventing system crashes.

Finally, typing on a keyboard shouldn't freeze the system. Instead of doing slow processing inside the interrupt handler (which can cause deadlocks if a lock is held), our keyboard handler only grabs the raw scancode from hardware port `0x60`, pushes it into a fast, lock-free ring buffer, and finishes. The kernel's main loop decodes and prints the keys safely outside interrupt context.

### Technical Architecture & Data Flow

```
                      +-----------------------------------+
                      |         PS/2 Keyboard             |
                      +-----------------------------------+
                                        | IRQ 1
                                        v
                      +-----------------------------------+
                      |        PIC 8259 (Remapped)        |
                      +-----------------------------------+
                                        | Vector 33
                                        v
                 +---------------------------------------------+
                 |       keyboard_interrupt_handler (ISR)       |
                 |  - Read port 0x60                           |
                 |  - Push scancode to lock-free SPSC queue    |
                 |  - Send EOI to PIC                          |
                 +---------------------------------------------+
                                        |
                             Lock-Free ScancodeQueue
                             (Atomic ring buffer)
                                        |
                                        v
                 +---------------------------------------------+
                 |         Main Loop (Outside ISR)             |
                 |  - Pop scancode                             |
                 |  - Decode with pc-keyboard                  |
                 |  - Echo to VGA Buffer & Serial (deadlock-free)
                 +---------------------------------------------+
```

### Emergency Stack Switching (IST)

```
Normal Execution (Kernel Stack)
           |
       Recursion!
           |
       Hits Stack Boundary / Guard Page -> Page Fault
           |
       Cannot push Page Fault frame to broken stack!
           |
           v
       Double Fault (Vector 8)
           |
           | [CPU reads IST index 0 from TSS]
           v
Switches to Emergency Stack (20 KiB dedicated buffer)
           |
           v
double_fault_handler executes safely!
```

---

## 4. How to Build, Run, and Test

### Prerequisites
Ensure QEMU is on your PATH:
```powershell
$env:PATH += ";C:\Program Files\qemu"
```

### Build Kernel
```powershell
cargo build
```

### Run Unit and Integration Tests
```powershell
cargo test
```
This automatically compiles and runs both the standard library tests and the dedicated `stack_overflow` integration test.

### Run in QEMU
```powershell
cargo run
```

---

## 5. Verification Evidence

### 1. `cargo build`
```text
   Compiling micro_os v0.1.0 (C:\Users\Venugopal\OneDrive\Desktop\micro_os-main)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.33s
```
*Result: Clean compilation, 0 warnings, 0 errors.*

### 2. `cargo test` Output (captured via QEMU isa-debug-exit)
```text
Running 4 tests
micro_os::interrupts::test_breakpoint_exception...	[ok]
micro_os::trivial_assertion...	[ok]
micro_os::vga_buffer::test_println_many...	[ok]
micro_os::vga_buffer::test_println_simple...	[ok]
     Running unittests src\main.rs
Serial port works too!
BOOT_TIME_MS=41.896 (TSC ticks=104952366, freq=2505 MHz)
Kernel ready. Interrupts OK.
Running 0 tests
     Running tests\stack_overflow.rs
stack_overflow::stack_overflow...	[ok]
```
*Result: All 4 unit tests passed, and the `stack_overflow` double-fault IST test succeeded.*

### 3. Live Hardware Keyboard Interrupt Demonstration
Sent `h`, `i`, `Enter` via QEMU monitor into the PS/2 controller:
```text
Serial port works too!
BOOT_TIME_MS=44.327 (TSC ticks=110548683, freq=2493 MHz)
Kernel ready. Interrupts OK.
hi
```
*Result: Keycodes were received by the ISR, placed into the lock-free queue, decoded, and echoed to serial and VGA output.*

---

## 6. Boot Time Measurement

- **Calibrated CPU TSC Frequency:** ~2,493 MHz to 2,506 MHz (calibrated dynamically using PIT Channel 2 over 10 ms).
- **Kernel Boot Time:** `41.896 ms` (from `_start` to interrupt initialization completion).
- **QEMU Total Boot Time:** ~1.4 s.

---

## 7. Known Limitations and What Is Next

### Known Limitations
1. **Memory Management is not initialized yet**: All allocations so far are static; the dynamic heap (`Box`, `Vec`, `String`) is not available until Step 2.
2. **Identity-Mapped Paging**: We still rely on the bootloader's initial page tables.

### What Is Next: Step 2 — Memory Management
- Access the active level 4 page table using the bootloader's physical memory mapping (`map_physical_memory` feature).
- Implement a `BootInfoFrameAllocator` tracking usable frames from `bootloader::bootinfo::MemoryMap`.
- Map kernel heap virtual memory pages to physical frames.
- Implement a memory allocator (e.g. linked-list allocator) and enable the Rust `alloc` crate.
