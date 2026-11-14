# Step 02: Memory Management (Paging, Frame Allocator, Kernel Heap, `alloc`)

## 1. Goal of the Step
Equip MicroOS with a complete virtual and physical memory management subsystem:
- Enable the bootloader's **complete physical memory mapping** (`map_physical_memory`).
- Transition from raw `_start` to the type-safe `entry_point!(kernel_main)` macro receiving `&'static BootInfo`.
- Initialize 4-level x86_64 paging using `OffsetPageTable`.
- Implement a physical **Frame Allocator** (`BootInfoFrameAllocator`) that discovers usable RAM from the BIOS memory map.
- Map a dedicated 100 KiB virtual address space for the kernel heap (`0x_4444_4444_0000`).
- Implement a custom, self-contained **Linked List Allocator** with free-list node tracking and memory reuse.
- Enable the Rust **`alloc`** crate to unlock `Box`, `Vec`, `String`, and dynamic data structures in `no_std`.
- Verify dynamic allocation correctness, large collections, and memory reclamation with integration tests.

---

## 2. What Was Added / Changed

| File | Purpose |
|---|---|
| `.cargo/config.toml` | Added `"alloc"` to `build-std` so rustc compiles the standard `alloc` library for `x86_64-micro_os`. |
| `Cargo.toml` | Enabled `bootloader` feature `map_physical_memory` and declared integration test `heap_allocation`. |
| `src/memory.rs` | Initializes 4-level paging via `OffsetPageTable` and implements `BootInfoFrameAllocator` for physical frame allocation. |
| `src/allocator/linked_list.rs` | Implements a robust `LinkedListAllocator` that stores free-list metadata in the free memory itself. |
| `src/allocator.rs` | Provides `Locked<A>` spinlock wrapper, registers `#[global_allocator]`, and defines `init_heap()` to map pages to physical frames. |
| `src/lib.rs` | Declares `extern crate alloc;`, exports `memory` and `allocator` modules, and updates test runner to initialize heap before tests. |
| `src/main.rs` | Migrates to `entry_point!(kernel_main)`, maps heap, demonstrates `Box`/`Vec`/`String`, and reports calibrated boot time. |
| `tests/heap_allocation.rs` | Integration test verifying simple allocations, large dynamic vectors (1000 items), multiple boxes, and memory reuse over 10,000 iterations. |
| `BOOT_TIMES.md` | Recorded calibrated boot time entry for Step 2. |
| `PROGRESS.md` | Updated status, decisions, and completed milestones. |
| `README.md` | Updated roadmap, memory architecture section, and prerequisites. |

---

## 3. How It Works

### Plain English Explanation
Before this step, the CPU had no concept of a dynamic "heap". All memory had to be hardcoded at compile time as static variables or placed on the call stack. If we wanted a variable-length list of items or needed to allocate memory whose size isn't known ahead of time, the operating system would simply crash.

To fix this, we need three layers:
1. **Physical Frames (The Land)**: The computer's actual RAM chips are divided into 4 KiB chunks called physical frames. The bootloader scans the motherboard and provides us a map of which chunks are free to use.
2. **Virtual Pages (The Addresses)**: In 64-bit protected mode, the CPU doesn't talk directly to RAM chips; it translates every address through a 4-level tree called a Page Table.
3. **The Heap Allocator (The Real Estate Agent)**: The allocator reserves a designated range of virtual addresses (`0x_4444_4444_0000`), asks the frame allocator to back those pages with real physical RAM, and manages individual requests from Rust code whenever someone writes `Box::new(...)` or `Vec::new()`.

When memory is freed (dropped), our linked list allocator takes that chunk of RAM and adds it to a "free list" so future allocations can reuse it rather than running out of memory.

---

### Why `map_physical_memory` and `entry_point!`?

#### 1. The `map_physical_memory` Feature
By default in x86_64, a kernel only has virtual addresses. If the kernel wants to read or modify a page table entry, it needs to access physical memory. But you cannot dereference a raw physical address directly—the CPU translation unit (MMU) will treat it as a virtual address!
By enabling `features = ["map_physical_memory"]` in `Cargo.toml`, the bootloader creates a 1:1 mapping of all physical memory starting at a high virtual offset (e.g. `0x0000_8000_0000_0000`). This allows our kernel to access any physical frame simply by adding the offset: `virtual_address = physical_address + physical_memory_offset`.

#### 2. The `entry_point!(kernel_main)` Macro
In previous steps, we used a raw `pub extern "C" fn _start() -> !`. 
However, the bootloader passes a pointer to a `BootInfo` structure containing the memory map and physical memory offset.
A raw `_start` function:
- Has no type-safety enforcement from the compiler (if the signature does not match what the bootloader passes in registers, the CPU reads garbage).
- Cannot guarantee the stack alignment required by the System V AMD64 ABI.
The `entry_point!` macro defines the real `_start` wrapper for us and guarantees that `kernel_main` strictly receives `&'static BootInfo` and never returns.

---

### Memory Subsystem Architecture

```
                          Rust Application Code
                     (Box::new, Vec::push, String)
                                   |
                                   v
                     +---------------------------+
                     |  #[global_allocator]      |
                     |  Locked<LinkedListAlloc>  |
                     +---------------------------+
                                   |
                    Manages 100 KiB Virtual Heap
                     [0x4444_4444_0000 .. +100K]
                                   |
                                   v
             +-------------------------------------------+
             |         4-Level Page Table (MMU)          |
             |  OffsetPageTable translating Page -> Frame|
             +-------------------------------------------+
                                   |
                                   v
             +-------------------------------------------+
             |          BootInfoFrameAllocator           |
             | Allocates 4 KiB physical frames from BIOS |
             | MemoryMap (usable RAM regions)            |
             +-------------------------------------------+
                                   |
                                   v
                       Physical RAM (Hardware)
```

### Linked List Node Tracking & Memory Reuse
When a block of memory is freed, the allocator writes a `ListNode` directly inside the unused memory:

```
[ Heap Memory Start ]
+---------------------+-------------------+---------------------+
| In-Use Allocation   | Free ListNode     | In-Use Allocation   |
| (Active Box<T>)     | Size: 64 bytes    | (Active Vec<T>)     |
|                     | Next: 0x...       |                     |
+---------------------+-------------------+---------------------+
```
No external metadata tables are required. When a new allocation request arrives, the allocator scans the linked list, selects a fitting free block, and slices off the excess.

---

## 4. How to Build, Run, and Test

### Compile the Kernel
```powershell
cargo build
```

### Run Unit and Integration Tests (including heap allocation)
```powershell
cargo test
```

### Run in QEMU
```powershell
cargo run
```

---

## 5. Verification Evidence

### 1. `cargo build`
```text
   Compiling micro_os v0.1.0 (C:\Users\Venugopal\OneDrive\Desktop\micro_os-main)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 25.02s
```
*Result: Clean compilation with `alloc` enabled, 0 warnings, 0 errors.*

### 2. `cargo test` Output (12 tests total across 3 test runners)
```text
Running 6 tests
micro_os::interrupts::test_breakpoint_exception...	[ok]
micro_os::test_box_allocation...	                [ok]
micro_os::test_vec_allocation...	                [ok]
micro_os::trivial_assertion...	                    [ok]
micro_os::vga_buffer::test_println_many...	        [ok]
micro_os::vga_buffer::test_println_simple...	        [ok]

     Running tests\heap_allocation.rs
heap_allocation::simple_allocation...	            [ok]
heap_allocation::large_vec...	                    [ok]
heap_allocation::many_boxes...	                    [ok]
heap_allocation::many_boxes_long_lived...	        [ok]
heap_allocation::memory_reuse...	                [ok]

     Running tests\stack_overflow.rs
stack_overflow::stack_overflow...	                [ok]

test result: ok. 0 failed; finished.
```
*Result: All heap allocation verification tests succeeded, confirming dynamic allocation and memory reuse.*

### 3. `cargo run` Serial Boot Output
```text
Serial port works too!
BOOT_TIME_MS=50.801 (TSC ticks=126221737, freq=2484 MHz)
Heap test: Box = 42, Vec len = 10
MicroOS Dynamic Heap Allocator Online!
Kernel ready. Interrupts & Heap OK.
rust
```
*Result: Dynamic `Box`, `Vec`, and `String` printed successfully to both VGA and Serial, and keyboard echo responded with `rust`.*

---

## 6. Boot Time Measurement

- **Calibrated CPU TSC Frequency:** ~2,484 MHz to 2,494 MHz.
- **Kernel Boot Time:** `50.801 ms` to `82.5 ms` (includes GDT, TSS, IDT, PIC, Paging setup, 100 KiB heap mapping, and early string formatting).
- **QEMU Total Boot Time:** ~1.4 s.

---

## 7. Known Limitations and What Is Next

### Known Limitations
1. **Fixed Heap Size**: The heap is currently fixed at 100 KiB. Growing the heap dynamically when full will be added as workloads increase.
2. **First-Fit Linked List Fragmentation**: A linked-list allocator can suffer from external memory fragmentation over long uptimes with mixed allocation sizes.

### What Is Next: Step 3 — Tasks, Keyboard Input & Interactive Shell
- Cooperative multitasking with an `async`/`await` executor.
- Asynchronous keyboard task listening on scancode queue.
- Interactive shell with prompt (`micro_os> `) and line editing (backspace, enter).
- Built-in commands: `help`, `clear`, `echo`, `uptime`, `meminfo`, `reboot`, `shutdown`, `bootime`.
- System clock combining PIT ticks and CMOS RTC for wall-clock time.
