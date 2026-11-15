#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;

use micro_os::cpu::get_cpu_info;
use micro_os::fs::vfs::VFS;
use micro_os::net::wifi::{SimulatedWiFi, WiFiDriver, WiFiState};
use micro_os::pkg::deb::{build_deb, parse_deb};
use micro_os::pkg::dpkg::DpkgDatabase;
use micro_os::pkg::rfc822::PackageMeta;
use micro_os::snap::{init_snap, SNAP_MANAGER};
use micro_os::storage::RamDisk;
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

    serial_print!("smoke::1_cpu_and_hardware_profile...\t");
    test_smoke_hardware();
    serial_println!("[ok]");

    serial_print!("smoke::2_storage_and_microfs_hierarchy...\t");
    test_smoke_filesystem();
    serial_println!("[ok]");

    serial_print!("smoke::3_wifi_and_networking_subsystems...\t");
    test_smoke_networking();
    serial_println!("[ok]");

    serial_print!("smoke::4_debian_apt_dpkg_package_manager...\t");
    test_smoke_apt_dpkg();
    serial_println!("[ok]");

    serial_print!("smoke::5_snap_store_containers_and_sandbox...\t");
    test_smoke_snap_store();
    serial_println!("[ok]");

    serial_println!("==================================================");
    serial_println!("MicroOS Full End-to-End Smoke Verification: PASSED");
    serial_println!("==================================================");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

/// 1. CPU & Hardware Profile Discovery
fn test_smoke_hardware() {
    let cpu = get_cpu_info();
    assert!(!cpu.vendor.is_empty(), "Vendor must not be empty");
    assert!(!cpu.brand.is_empty(), "Brand must not be empty");
    assert!(cpu.logical_cores >= 1);
    assert!(cpu.features.contains(&"tsc"));
    assert!(cpu.features.contains(&"sse2"));
}

/// 2. Storage & Filesystem Hierarchy (MicroFS on RAM Disk)
fn test_smoke_filesystem() {
    let disk = Box::new(RamDisk::new(64 * 1024));
    let mut vfs = VFS.lock();
    vfs.set_device(disk);
    vfs.format().expect("format failed");

    // Create nested directory and file
    vfs.make_dir("/etc").expect("mkdir /etc");
    vfs.write_file("/etc/os-release", b"NAME=MicroOS\nVERSION=1.0\n").expect("write os-release");

    let read_data = vfs.read_file("/etc/os-release").expect("read os-release");
    assert_eq!(read_data, b"NAME=MicroOS\nVERSION=1.0\n");
}

/// 3. WiFi & Networking Subsystem
fn test_smoke_networking() {
    let mut wifi = SimulatedWiFi::new([0x52, 0x54, 0x00, 0x12, 0x34, 0x56]);
    let scan_results = wifi.scan();
    assert!(scan_results.len() >= 4);

    let lab_ap = scan_results.iter().find(|ap| ap.ssid == "MicroOS-Lab-5G").expect("find AP");
    assert_eq!(lab_ap.channel, 36);

    let conn_res = wifi.connect("MicroOS-Lab-5G", Some("microos2026"));
    assert!(conn_res.is_ok());
    assert_eq!(wifi.status().state, WiFiState::Connected);

    let _ = wifi.disconnect();
    assert_eq!(wifi.status().state, WiFiState::Disconnected);
}

/// 4. Debian apt/dpkg Package Management
fn test_smoke_apt_dpkg() {
    let meta = PackageMeta::new("smoke-tool", "1.0.0");
    let control = "Package: smoke-tool\nVersion: 1.0.0\nArchitecture: x86_64\nDescription: Smoke tool\n";
    let payload = b"SMOKE_BINARY_BYTES";
    let deb_bytes = build_deb(control, &[("bin/smoke", payload)]);
    let deb = parse_deb(&deb_bytes).expect("parse deb");

    let mut db = DpkgDatabase::new();
    db.install(&deb, &meta).expect("install deb");
    assert!(db.is_installed("smoke-tool"));

    let read_file = VFS.lock().read_file("/bin/smoke").expect("read unpacked binary");
    assert_eq!(read_file, payload);

    db.remove("smoke-tool").expect("remove deb");
    assert!(!db.is_installed("smoke-tool"));
    assert!(VFS.lock().read_file("/bin/smoke").is_err());
}

/// 5. Snap Store Containers & Sandbox Execution
fn test_smoke_snap_store() {
    init_snap();

    let mut lock = SNAP_MANAGER.lock();
    let mgr = lock.as_mut().unwrap();

    let search_res = mgr.find("calc");
    assert!(!search_res.is_empty());

    mgr.install("micro-calc", None, false).expect("install micro-calc");
    assert_eq!(mgr.list().len(), 1);

    let run_res = mgr.run("micro-calc", &["20", "+", "22"]).expect("run micro-calc");
    assert!(run_res.contains("20 + 22 = 42"));

    mgr.remove("micro-calc").expect("remove micro-calc");
    assert!(mgr.list().is_empty());
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
