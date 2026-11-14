#![no_std]
#![no_main]

extern crate alloc;

use alloc::borrow::ToOwned;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::net::wifi::{
    AccessPoint, Bssid, FrameControl80211, Header80211, SimulatedWiFi, WiFiAuthType, WiFiDriver,
    WiFiError, WiFiState,
};
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

    serial_print!("wifi::ap_properties...\t");
    test_ap_properties();
    serial_println!("[ok]");

    serial_print!("wifi::frame_control_and_headers...\t");
    test_frame_control_and_headers();
    serial_println!("[ok]");

    serial_print!("wifi::scanning...\t");
    test_wifi_scanning();
    serial_println!("[ok]");

    serial_print!("wifi::state_machine_and_wpa2...\t");
    test_wifi_state_machine();
    serial_println!("[ok]");

    serial_print!("wifi::bridged_frames...\t");
    test_wifi_bridged_frames(&mut mapper, &mut frame_allocator);
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_ap_properties() {
    let ap_24g = AccessPoint {
        ssid: "Test-2.4G".to_owned(),
        bssid: Bssid([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]),
        channel: 6,
        rssi: -65,
        auth: WiFiAuthType::Open,
        bitrate_mbps: 300,
    };
    assert_eq!(ap_24g.frequency_mhz(), 2437);
    assert_eq!(ap_24g.link_quality(), (49, 70));

    let ap_5g = AccessPoint {
        ssid: "Test-5G".to_owned(),
        bssid: Bssid([0x00, 0x11, 0x22, 0x33, 0x44, 0x66]),
        channel: 36,
        rssi: -42,
        auth: WiFiAuthType::Wpa2Psk,
        bitrate_mbps: 866,
    };
    assert_eq!(ap_5g.frequency_mhz(), 5180);
    assert_eq!(ap_5g.link_quality(), (70, 70));

    let ap_weak = AccessPoint {
        ssid: "Weak-Signal".to_owned(),
        bssid: Bssid([0x00, 0x11, 0x22, 0x33, 0x44, 0x77]),
        channel: 1,
        rssi: -105,
        auth: WiFiAuthType::Wpa3Sae,
        bitrate_mbps: 54,
    };
    assert_eq!(ap_weak.frequency_mhz(), 2412);
    assert_eq!(ap_weak.link_quality(), (0, 70));
}

fn test_frame_control_and_headers() {
    // Standard 802.11 MAC header with 3 addresses is 24 bytes
    assert_eq!(core::mem::size_of::<Header80211>(), 24);

    let fc = FrameControl80211::new(0x00, 0x08, 0x01); // Management Beacon frame, to_ds flag
    assert_eq!(fc.frame_type(), 0x00);
    assert_eq!(fc.subtype(), 0x08);

    let bssid = Bssid([0x00, 0x14, 0xD1, 0x55, 0xAA, 0x01]);
    let _client = [0x50, 0x54, 0x00, 0x12, 0x34, 0x56];
    let broadcast = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

    let header = Header80211 {
        frame_control: fc,
        duration: 0x013A,
        addr1: broadcast,
        addr2: bssid.0,
        addr3: bssid.0,
        seq_ctrl: 0x1230,
    };

    assert_eq!(header.addr1, [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(header.addr2, bssid.0);
    assert_eq!(header.addr3, bssid.0);
}

fn test_wifi_scanning() {
    let mut adapter = SimulatedWiFi::new([0x50, 0x54, 0x00, 0x12, 0x34, 0x56]);
    assert_eq!(adapter.name(), "wlan0");
    assert_eq!(adapter.mac_address(), [0x50, 0x54, 0x00, 0x12, 0x34, 0x56]);
    assert_eq!(adapter.status().state, WiFiState::Disconnected);

    let scan_results = adapter.scan();
    assert_eq!(scan_results.len(), 4);

    let lab = scan_results.iter().find(|ap| ap.ssid == "MicroOS-Lab-5G");
    assert!(lab.is_some());
    let lab = lab.unwrap();
    assert_eq!(lab.channel, 36);
    assert_eq!(lab.auth, WiFiAuthType::Wpa2Psk);
    assert_eq!(lab.bitrate_mbps, 866);

    let guest = scan_results.iter().find(|ap| ap.ssid == "Guest-WiFi-Open");
    assert!(guest.is_some());
    assert_eq!(guest.unwrap().auth, WiFiAuthType::Open);
}

fn test_wifi_state_machine() {
    let mut adapter = SimulatedWiFi::new([0x50, 0x54, 0x00, 0x12, 0x34, 0x56]);

    // 1. Unknown SSID
    let res = adapter.connect("NonExistentSSID", None);
    assert_eq!(res, Err(WiFiError::SsidNotFound));
    assert_eq!(adapter.status().state, WiFiState::Disconnected);

    // 2. WPA2-PSK with incorrect passphrase
    let res = adapter.connect("MicroOS-Lab-5G", Some("wrong_password"));
    assert_eq!(res, Err(WiFiError::AuthenticationFailed));
    assert_eq!(adapter.status().state, WiFiState::Disconnected);
    assert!(adapter.status().ssid.is_none());

    // 3. WPA2-PSK with valid passphrase
    let res = adapter.connect("MicroOS-Lab-5G", Some("microos2026"));
    assert_eq!(res, Ok(()));
    assert_eq!(adapter.status().state, WiFiState::Connected);
    assert_eq!(adapter.status().ssid, Some("MicroOS-Lab-5G".to_owned()));
    assert_eq!(adapter.status().channel, 36);
    assert_eq!(adapter.status().bitrate_mbps, 866);

    // 4. Connecting while already connected to same network
    let res = adapter.connect("MicroOS-Lab-5G", Some("microos2026"));
    assert_eq!(res, Err(WiFiError::AlreadyConnected));

    // 5. Connect to Open network (seamless switch)
    let res = adapter.connect("Guest-WiFi-Open", None);
    assert_eq!(res, Ok(()));
    assert_eq!(adapter.status().state, WiFiState::Connected);
    assert_eq!(adapter.status().ssid, Some("Guest-WiFi-Open".to_owned()));
    assert_eq!(adapter.status().channel, 6);

    // 6. Disconnect
    let res = adapter.disconnect();
    assert_eq!(res, Ok(()));
    assert_eq!(adapter.status().state, WiFiState::Disconnected);
    assert!(adapter.status().ssid.is_none());

    // 7. Disconnect when already disconnected
    let res = adapter.disconnect();
    assert_eq!(res, Err(WiFiError::NotConnected));
}

fn test_wifi_bridged_frames(
    mapper: &mut (impl x86_64::structures::paging::Mapper<x86_64::structures::paging::Size4KiB>
              + x86_64::structures::paging::Translate),
    frame_allocator: &mut impl x86_64::structures::paging::FrameAllocator<
        x86_64::structures::paging::Size4KiB,
    >,
) {
    let mut adapter = SimulatedWiFi::new([0x50, 0x54, 0x00, 0x12, 0x34, 0x56]);

    // Send frame when disconnected -> Error
    let dummy_frame = [0u8; 64];
    assert_eq!(adapter.send_frame(&dummy_frame), Err(WiFiError::NotConnected));
    assert_eq!(adapter.receive_frame(), None);

    // Initialize network so e1000 descriptor ring is ready
    micro_os::net::init_network(mapper, frame_allocator);

    // Connect to WiFi
    assert!(adapter.connect("MicroOS-Lab-5G", Some("microos2026")).is_ok());

    // Send frame through WiFi bridge
    let eth_packet = [
        0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // Dst
        0x50, 0x54, 0x00, 0x12, 0x34, 0x56, // Src
        0x08, 0x00,                         // IPv4
        0x45, 0x00, 0x00, 0x20,             // IP Header ...
        0x00, 0x01, 0x00, 0x00,
        0x40, 0x01, 0x00, 0x00,
        0x0A, 0x00, 0x02, 0x0F,
        0x0A, 0x00, 0x02, 0x02,
        0x08, 0x00, 0xF7, 0xFF,             // ICMP Echo
        0x00, 0x01, 0x00, 0x01,
    ];
    let res = adapter.send_frame(&eth_packet);
    assert!(res.is_ok());

    let status = adapter.status();
    assert_eq!(status.tx_packets, 1);
    assert_eq!(status.tx_bytes, eth_packet.len() as u64);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
