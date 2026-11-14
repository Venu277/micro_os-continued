use crate::gdt;
use crate::println;
use core::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// ---------- PIC 8259 setup ----------

/// PIC1 starts at interrupt vector 32 (right after CPU exceptions 0-31).
pub const PIC_1_OFFSET: u8 = 32;
/// PIC2 starts at interrupt vector 40.
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

pub static PICS: spin::Mutex<ChainedPics> =
    spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Hardware interrupt vector numbers (after PIC remapping).
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,        // IRQ 0 → vector 32
    Keyboard = PIC_1_OFFSET + 1, // IRQ 1 → vector 33
    PrimaryAta = PIC_1_OFFSET + 14,   // IRQ 14 → vector 46
    SecondaryAta = PIC_1_OFFSET + 15, // IRQ 15 → vector 47
}

impl InterruptIndex {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

// ---------- Lock-Free Scancode Queue (SPSC) ----------

const SCANCODE_QUEUE_CAPACITY: usize = 128;

pub struct ScancodeQueue {
    buffer: [AtomicU8; SCANCODE_QUEUE_CAPACITY],
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl ScancodeQueue {
    pub const fn new() -> Self {
        const ZERO: AtomicU8 = AtomicU8::new(0);
        Self {
            buffer: [ZERO; SCANCODE_QUEUE_CAPACITY],
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    /// Push a scancode into the queue. Wait-free and lock-free; safe in interrupt context.
    pub fn push(&self, scancode: u8) -> Result<(), ()> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= SCANCODE_QUEUE_CAPACITY {
            return Err(()); // Queue is full; drop scancode to avoid deadlock
        }
        self.buffer[head % SCANCODE_QUEUE_CAPACITY].store(scancode, Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Pop a scancode from the queue. Non-blocking; called outside interrupt context.
    pub fn pop(&self) -> Option<u8> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if head == tail {
            return None; // Queue is empty
        }
        let scancode = self.buffer[tail % SCANCODE_QUEUE_CAPACITY].load(Ordering::Relaxed);
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(scancode)
    }
}

pub static SCANCODE_QUEUE: ScancodeQueue = ScancodeQueue::new();

/// Pop a pending keyboard scancode from the lock-free queue.
pub fn pop_scancode() -> Option<u8> {
    SCANCODE_QUEUE.pop()
}

// ---------- Timer Tick Accounting ----------

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);

/// Get the current number of timer interrupt ticks elapsed.
pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Relaxed)
}

// ---------- PIT TSC Calibration ----------

/// Calibrates the CPU TSC frequency using PIT Channel 2 one-shot mode over ~10 ms.
/// Returns estimated TSC frequency in Hz.
pub fn calibrate_tsc() -> u64 {
    // PIT operates at 1,193,182 Hz. 10 ms is ~11,932 counter ticks.
    let mut pit_cmd: Port<u8> = Port::new(0x43);
    let mut pit_ch2: Port<u8> = Port::new(0x42);
    let mut port_61: Port<u8> = Port::new(0x61);

    unsafe {
        // Disable Channel 2 gate and speaker output
        let orig = port_61.read();
        port_61.write(orig & !0x03);

        // PIT Mode: Channel 2, Access mode lobyte/hibyte, Mode 0 (interrupt on terminal count)
        // 0b10110000 = 0xB0
        pit_cmd.write(0xB0);

        let count: u16 = 11932; // ~10 ms (11932 / 1193182 = 0.01000015 s)
        pit_ch2.write((count & 0xFF) as u8);
        pit_ch2.write((count >> 8) as u8);

        // Start timer by asserting Channel 2 gate (bit 0 = 1)
        port_61.write((orig & !0x02) | 0x01);

        let tsc_start = core::arch::x86_64::_rdtsc();

        // Spin until Channel 2 OUT (bit 5 of port 0x61) transitions to high
        let mut timeout = 10_000_000usize;
        while (port_61.read() & 0x20) == 0 && timeout > 0 {
            timeout -= 1;
            core::hint::spin_loop();
        }

        let tsc_end = core::arch::x86_64::_rdtsc();

        // Restore port 0x61 state
        port_61.write(orig & !0x03);

        if timeout == 0 {
            // Fallback if PIT Channel 2 didn't respond: default to 2.0 GHz
            return 2_000_000_000;
        }

        let elapsed_tsc = tsc_end.wrapping_sub(tsc_start);
        // Frequency (Hz) = elapsed_tsc / 0.01s = elapsed_tsc * 100
        let freq = elapsed_tsc.saturating_mul(100);
        if freq < 100_000_000 {
            2_000_000_000 // Fallback if unrealistic
        } else {
            freq
        }
    }
}

// ---------- IDT Setup ----------

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // CPU exceptions
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);

        // Hardware interrupts (PIC)
        idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler);
        idt[InterruptIndex::PrimaryAta.as_usize()].set_handler_fn(ata_primary_interrupt_handler);
        idt[InterruptIndex::SecondaryAta.as_usize()].set_handler_fn(ata_secondary_interrupt_handler);

        idt
    };
}

/// Load the Interrupt Descriptor Table into the CPU IDTR.
pub fn init_idt() {
    IDT.load();
}

// ---------- CPU Exception Handlers ----------

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed address: {:?}", Cr2::read());
    println!("Error code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    crate::hlt_loop();
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!(
        "EXCEPTION: GENERAL PROTECTION FAULT (code {:#x})\n{:#?}",
        error_code, stack_frame
    );
}

// ---------- Hardware Interrupt Handlers ----------

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    let mut port = Port::new(0x60); // PS/2 data port
    let scancode: u8 = unsafe { port.read() };

    // Push into lock-free queue; non-blocking, deadlock-free
    let _ = SCANCODE_QUEUE.push(scancode);

    // Wake async keyboard executor task if sleeping
    crate::task::keyboard::wake_keyboard_task();

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn ata_primary_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::PrimaryAta.as_u8());
    }
}

extern "x86-interrupt" fn ata_secondary_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::SecondaryAta.as_u8());
    }
}

// ---------- Unit Tests ----------

#[test_case]
fn test_breakpoint_exception() {
    x86_64::instructions::interrupts::int3();
}
