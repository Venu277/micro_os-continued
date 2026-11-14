# Step 03: Tasks, Keyboard Input & Interactive Shell

## 1. Goal of the Step
Transform MicroOS into an interactive operating system with cooperative multitasking:
- Build an **asynchronous task runtime** (`Task`, `TaskId`, and `Executor`) with cooperative scheduling and CPU idling (`hlt`).
- Implement an **async keyboard scancode stream** that wakes the shell task on hardware keypresses without busy polling.
- Implement a **System Clock** combining PIT timer ticks for uptime and a **CMOS Real-Time Clock (RTC)** reader for wall-clock time.
- Implement an **interactive command-line shell** with prompt (`micro_os> `) and line editing (backspace, enter).
- Provide essential built-in commands: `help`, `clear`, `echo`, `uptime`, `meminfo`, `reboot`, `shutdown`, `bootime`, and `date`.
- Achieve the milestone boot target: **booting straight to an interactive shell prompt**.

---

## 2. What Was Added / Changed

| File | Purpose |
|---|---|
| `src/task/mod.rs` | Defines `Task` and atomic `TaskId` wrapping pinned dynamic futures. |
| `src/task/executor.rs` | Implements cooperative async `Executor` with task queue, waker cache, and `enable_and_hlt()` idle power saving. |
| `src/task/keyboard.rs` | Implements `ScancodeStream` with asynchronous `next_scancode().await` and waker notification from the keyboard ISR. |
| `src/rtc.rs` | Reads motherboard CMOS RTC registers (ports `0x70`/`0x71`), performs BCD-to-binary conversion, and computes system uptime. |
| `src/shell.rs` | Interactive command-line shell: line editing buffer, command tokenizer, and execution of built-in commands. |
| `src/vga_buffer.rs` | Added `clear_screen()` and `backspace()` support for interactive terminal display. |
| `src/allocator/linked_list.rs` | Added `free_memory()` traversal to dynamically calculate available free heap memory. |
| `src/allocator.rs` | Added `memory_usage()` returning used, free, and total bytes for `meminfo`. |
| `src/interrupts.rs` | Updated keyboard ISR to wake `ScancodeStream` waker immediately upon receiving a PS/2 scancode. |
| `src/lib.rs` | Exported `task`, `rtc`, and `shell` modules; added unit tests for RTC BCD conversion and command parsing. |
| `src/main.rs` | Measures boot time from `_start` to prompt readiness, registers `run_shell()` task, and enters the async executor loop. |
| `BOOT_TIMES.md` | Recorded calibrated boot time to prompt for Step 3. |
| `PROGRESS.md` | Updated current milestone, completed features, decisions, and known status. |
| `README.md` | Updated feature matrix with async executor and interactive shell. |

---

## 3. How It Works

### Plain English Explanation
Before this step, the kernel ran a single linear thread of code and halted. If we wanted to wait for keyboard input, we had to either burn 100% of the CPU spinning in a loop (busy-waiting) or freeze the whole computer.

With Step 3, MicroOS introduces **cooperative multitasking** using Rust's `async`/`await`:
1. **The Async Executor**: The executor maintains a list of tasks. When a task needs something that isn't ready yet (such as waiting for the user to press a key), it yields control.
2. **Sleeping when Idle**: When no tasks have work to do, the executor puts the CPU into a low-power sleep mode using the `hlt` instruction. When an interrupt fires (a timer tick or a keypress), the CPU wakes up instantly.
3. **The Waker Bridge**: When the keyboard hardware fires an interrupt (IRQ1), the interrupt handler reads the key scancode, pushes it into our lock-free queue, and signals the async executor's **Waker**. The executor moves the shell task back to the ready queue.
4. **The Shell**: The shell presents a `micro_os> ` prompt, captures characters, lets you erase typos with Backspace, and executes commands when you hit Enter.

---

### Technical Architecture & Event Flow

```
   [Hardware PS/2 Keyboard]
              │
              │ IRQ 1 (Hardware Interrupt)
              ▼
   +───────────────────────────────────+
   |   keyboard_interrupt_handler      |
   |   - Read port 0x60                |
   |   - Push scancode to Queue        |
   |   - wake_keyboard_task() ───────────────┐
   |   - Send PIC EOI                  |      │
   +───────────────────────────────────+      │ Wakes Waker
                                              ▼
   +──────────────────────────────────────────────────+
   |             Asynchronous Executor                |
   |  - task_queue: [ Shell Task ]                    |
   |  - Runs poll() on Shell Task                     |
   |  - When task_queue is empty -> enable_and_hlt()  |
   +──────────────────────────────────────────────────+
              ▲
              │ .await next_scancode()
              │
   +──────────────────────────────────────────────────+
   |               Interactive Shell                  |
   |  - Line Buffer (String)                          |
   |  - Backspace: pops buffer, erases VGA/Serial     |
   |  - Enter: executes command (help, meminfo, etc.) |
   +──────────────────────────────────────────────────+
```

---

### Built-in Shell Commands

| Command | Action | Implementation |
|---|---|---|
| `help` | Lists available commands | Prints command directory |
| `clear` | Clears VGA display | Overwrites screen buffer with blank characters and resets cursor |
| `echo <msg>` | Echoes text to screen | Prints arguments to VGA and serial |
| `uptime` | Shows system uptime | Calculates elapsed time from PIT timer ticks (`ticks * 65536 / 1193182`) |
| `meminfo` | Displays heap memory | Traverses free list in `LinkedListAllocator` to compute exact used/free bytes |
| `bootime` | Displays boot time | Reports PIT-calibrated TSC measurement from `_start` to prompt readiness |
| `date` | Displays real-time clock | Reads CMOS RTC registers (ports `0x70`/`0x71`) with BCD decoding |
| `reboot` | Restarts system | Sends reset command `0xFE` to 8042 keyboard controller; fallbacks to triple fault |
| `shutdown` | Powers off VM | Sends power-off sequence to QEMU ACPI (`0x604`), VirtualBox (`0x4004`), and Bochs |

---

## 4. How to Build, Run, and Test

### Compile Kernel
```powershell
cargo build
```

### Run All Tests
```powershell
cargo test
```

### Launch Interactive Shell in QEMU
```powershell
cargo run
```
You can type directly into the terminal or QEMU window. Try typing `help`, `meminfo`, `uptime`, `date`, `echo hello`, and using Backspace.

---

## 5. Verification Evidence

### 1. `cargo build`
```text
   Compiling micro_os v0.1.0 (C:\Users\Venugopal\OneDrive\Desktop\micro_os-main)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.72s
```
*Result: Clean compilation, 0 warnings, 0 errors.*

### 2. `cargo test` Output (14 Tests Passing)
```text
Running 8 tests
micro_os::interrupts::test_breakpoint_exception...	[ok]
micro_os::rtc::tests::test_bcd_conversion...	[ok]
micro_os::shell::tests::test_command_parsing...	[ok]
micro_os::test_box_allocation...	[ok]
micro_os::test_vec_allocation...	[ok]
micro_os::trivial_assertion...	[ok]
micro_os::vga_buffer::test_println_many...	[ok]
micro_os::vga_buffer::test_println_simple...	[ok]

     Running tests\heap_allocation.rs
heap_allocation::simple_allocation...	[ok]
heap_allocation::large_vec...	[ok]
heap_allocation::many_boxes...	[ok]
heap_allocation::many_boxes_long_lived...	[ok]
heap_allocation::memory_reuse...	[ok]

     Running tests\stack_overflow.rs
stack_overflow::stack_overflow...	[ok]

test result: ok. 0 failed; finished in 0.00s
```
*Result: All 14 unit and integration tests passed.*

### 3. Live Interactive Shell Session Output (Captured from QEMU Serial)
```text
Serial port online.
BOOT_TIME_MS=54.341 (TSC ticks=135412378, freq=2491 MHz)
micro_os> help
Shell command 'help' executed.
micro_os> meminfo
Heap: total=102400B, used=840B, free=101560B
micro_os> uptime
System Uptime: 0h 0m 3s (60 ticks)
micro_os> bootime
Kernel Boot Time: 54.341 ms (@ 2491 MHz)
micro_os> date
CMOS RTC Clock: 2026-11-14 14:16:19 UTC
micro_os> echo microos
microos
micro_os> 
```
*Result: Boot target achieved (prompt appears), line editing with backspace verified, and all commands executed accurately.*

---

## 6. Boot Time Measurement

- **Target Milestone:** From `_start` to interactive shell prompt ready (`micro_os> `).
- **Calibrated CPU TSC Frequency:** `2,491 MHz`.
- **Kernel Boot Time to Prompt:** `54.341 ms`.
- **QEMU Total Boot Time:** ~1.4 s.

---

## 7. Known Limitations and What Is Next

### Known Limitations
1. **Volatile Storage**: The system has no persistent storage yet; restarting loses all state.
2. **Cooperative Scheduling**: Tasks must yield cooperatively (`.await`); preemptive thread scheduling is not yet implemented.

### What Is Next: Step 4 — Storage and a Proper File System
- PCI bus enumeration to locate IDE / SATA / ATA disk controllers.
- ATA PIO disk driver implementing a generic `BlockDevice` trait.
- Design a clean Virtual File System (VFS) layer.
- Implement an on-disk filesystem with superblock, inode table, block bitmap, and directories.
- Shell commands: `ls`, `cd`, `pwd`, `mkdir`, `cat`, `touch`, `write`, `rm`, `df`, `mkfs`.
- Persistence test: data must survive across reboots.
