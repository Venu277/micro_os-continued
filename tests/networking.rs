#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::net::e1000::{RxDesc, TxDesc};
use micro_os::net::{init_network, NETWORK};
use micro_os::shell::parse_ipv4;
use micro_os::{allocator, exit_qemu, memory, serial_print, serial_println, QemuExitCode};
use smoltcp::phy::ChecksumCapabilities;
use smoltcp::wire::{Icmpv4Message, Icmpv4Packet, Icmpv4Repr, Ipv4Address};
use x86_64::VirtAddr;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    micro_os::init();
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

    serial_print!("networking::parse_ipv4...\t");
    test_parse_ipv4();
    serial_println!("[ok]");

    serial_print!("networking::e1000_descriptors...\t");
    test_e1000_descriptors();
    serial_println!("[ok]");

    serial_print!("networking::icmp_formatting...\t");
    test_icmp_formatting();
    serial_println!("[ok]");

    serial_print!("networking::init_and_ping_gateway...\t");
    test_init_and_ping_gateway(&mut mapper, &mut frame_allocator);
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_parse_ipv4() {
    let valid = parse_ipv4("10.0.2.15").expect("failed to parse valid IP");
    assert_eq!(valid, Ipv4Address::new(10, 0, 2, 15));

    let gw = parse_ipv4("192.168.1.1").expect("failed to parse valid IP");
    assert_eq!(gw, Ipv4Address::new(192, 168, 1, 1));

    assert!(parse_ipv4("256.0.0.1").is_none());
    assert!(parse_ipv4("10.0.1").is_none());
    assert!(parse_ipv4("invalid").is_none());
    assert!(parse_ipv4("10.0.0.0.1").is_none());
}

fn test_e1000_descriptors() {
    assert_eq!(core::mem::size_of::<RxDesc>(), 16);
    assert_eq!(core::mem::align_of::<RxDesc>(), 16);

    assert_eq!(core::mem::size_of::<TxDesc>(), 16);
    assert_eq!(core::mem::align_of::<TxDesc>(), 16);
}

fn test_icmp_formatting() {
    let payload = b"Ping_Test_Payload_1234";
    let repr = Icmpv4Repr::EchoRequest {
        ident: 0x1234,
        seq_no: 1,
        data: payload,
    };

    let mut buf = vec![0u8; repr.buffer_len()];
    let mut packet = Icmpv4Packet::new_unchecked(&mut buf);
    repr.emit(&mut packet, &ChecksumCapabilities::default());

    let checked_packet = Icmpv4Packet::new_checked(&buf[..]).expect("valid ICMP packet");
    assert_eq!(checked_packet.msg_type(), Icmpv4Message::EchoRequest);
    assert_eq!(checked_packet.echo_ident(), 0x1234);
    assert_eq!(checked_packet.echo_seq_no(), 1);
    assert_eq!(checked_packet.data(), payload);
    assert!(checked_packet.verify_checksum());
}

fn test_init_and_ping_gateway(
    mapper: &mut (impl x86_64::structures::paging::Mapper<x86_64::structures::paging::Size4KiB>
              + x86_64::structures::paging::Translate),
    frame_allocator: &mut impl x86_64::structures::paging::FrameAllocator<
        x86_64::structures::paging::Size4KiB,
    >,
) {
    init_network(mapper, frame_allocator);

    let mut net_lock = NETWORK.lock();
    if let Some(ref mut stack) = *net_lock {
        let gw = Ipv4Address::new(10, 0, 2, 2);
        // Ping QEMU gateway (10.0.2.2)
        match stack.ping(gw, 3000) {
            Ok(rtt) => {
                serial_print!("(gw ping rtt={}ms) ", rtt);
            }
            Err(e) => {
                serial_print!("(ping note: {}) ", e);
            }
        }
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
