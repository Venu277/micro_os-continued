#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use x86_64::VirtAddr;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    serial_print!("heap_allocation::simple_allocation...\t");
    simple_allocation();
    serial_println!("[ok]");

    serial_print!("heap_allocation::large_vec...\t");
    large_vec();
    serial_println!("[ok]");

    serial_print!("heap_allocation::many_boxes...\t");
    many_boxes();
    serial_println!("[ok]");

    serial_print!("heap_allocation::many_boxes_long_lived...\t");
    many_boxes_long_lived();
    serial_println!("[ok]");

    serial_print!("heap_allocation::memory_reuse...\t");
    memory_reuse();
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn simple_allocation() {
    let heap_value_1 = Box::new(41);
    let heap_value_2 = Box::new(13);
    assert_eq!(*heap_value_1, 41);
    assert_eq!(*heap_value_2, 13);
}

fn large_vec() {
    let n = 1000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.iter().sum::<u64>(), (n - 1) * n / 2);
}

fn many_boxes() {
    for i in 0..allocator::HEAP_SIZE / 8 {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

fn many_boxes_long_lived() {
    let long_lived = Box::new(1);
    for i in 0..allocator::HEAP_SIZE / 8 {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    assert_eq!(*long_lived, 1);
}

fn memory_reuse() {
    for i in 0..10_000 {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
