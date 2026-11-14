#![no_std]
#![no_main]

extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::fs::vfs::VFS;
use micro_os::pkg::deb::{build_deb, parse_deb};
use micro_os::pkg::deps::{resolve_dependencies, DepError};
use micro_os::pkg::dpkg::{parse_status_file, DpkgDatabase};
use micro_os::pkg::rfc822::{parse_packages_index, PackageMeta};
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

    serial_print!("pkg_apt::rfc822_parser...\t");
    test_rfc822_parser();
    serial_println!("[ok]");

    serial_print!("pkg_apt::deb_container...\t");
    test_deb_container();
    serial_println!("[ok]");

    serial_print!("pkg_apt::dependency_resolution...\t");
    test_dependency_resolution();
    serial_println!("[ok]");

    serial_print!("pkg_apt::dpkg_status_and_unpacking...\t");
    test_dpkg_status_and_unpacking();
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_rfc822_parser() {
    let index_text = r#"
Package: test-pkg
Version: 2.1.0-1
Architecture: x86_64
Maintainer: Test Maintainer <test@example.org>
Depends: liba, libb (>= 1.0)
Filename: pool/main/t/test-pkg/test-pkg_2.1.0-1_x86_64.deb
Size: 4096
SHA256: 1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef
Section: utils
Priority: optional
Description: Test package for MicroOS
 A multi-line description text for testing
 RFC 822 continuation parsing.

Package: second-pkg
Version: 1.0.0
Architecture: x86_64
Description: Second standalone package
"#;

    let pkgs = parse_packages_index(index_text);
    assert_eq!(pkgs.len(), 2);

    let p1 = &pkgs[0];
    assert_eq!(p1.name, "test-pkg");
    assert_eq!(p1.version, "2.1.0-1");
    assert_eq!(p1.architecture, "x86_64");
    assert_eq!(p1.maintainer, "Test Maintainer <test@example.org>");
    assert_eq!(p1.depends, vec!["liba".to_owned(), "libb".to_owned()]);
    assert_eq!(p1.size, 4096);
    assert!(p1.description.contains("Test package for MicroOS"));
    assert!(p1.description.contains("A multi-line description text"));

    let p2 = &pkgs[1];
    assert_eq!(p2.name, "second-pkg");
    assert_eq!(p2.version, "1.0.0");
    assert!(p2.depends.is_empty());
}

fn test_deb_container() {
    let control = "Package: sample-app\nVersion: 1.0.0\nArchitecture: x86_64\nDescription: Sample App\n";
    let file1_data = b"#!/bin/sh\necho 'Hello MicroOS!'\n";
    let file2_data = b"config_option=true\n";

    let deb_bytes = build_deb(
        control,
        &[
            ("bin/sample", file1_data),
            ("etc/sample.conf", file2_data),
        ],
    );

    let deb = parse_deb(&deb_bytes).expect("Failed to parse built .deb archive");
    assert_eq!(deb.version, "2.0");
    assert!(deb.control_text.contains("Package: sample-app"));
    assert_eq!(deb.files.len(), 2);

    let bin_file = deb.files.iter().find(|f| f.path == "bin/sample").unwrap();
    assert_eq!(bin_file.data, file1_data);

    let conf_file = deb.files.iter().find(|f| f.path == "etc/sample.conf").unwrap();
    assert_eq!(conf_file.data, file2_data);
}

fn test_dependency_resolution() {
    let p_libc = PackageMeta::new("libc-micro", "1.0.0");
    let mut p_term = PackageMeta::new("libterm", "0.9.1");
    p_term.depends = vec!["libc-micro".to_owned()];

    let mut p_editor = PackageMeta::new("micro-editor", "1.2.0");
    p_editor.depends = vec!["libc-micro".to_owned(), "libterm".to_owned()];

    let available = vec![p_libc.clone(), p_term.clone(), p_editor.clone()];

    // 1. Resolve micro-editor from clean slate -> [libc-micro, libterm, micro-editor]
    let plan = resolve_dependencies("micro-editor", &available, &[]).expect("resolution failed");
    assert_eq!(plan.len(), 3);
    assert_eq!(plan[0].name, "libc-micro");
    assert_eq!(plan[1].name, "libterm");
    assert_eq!(plan[2].name, "micro-editor");

    // 2. Resolve micro-editor when libc-micro is already installed -> [libterm, micro-editor]
    let installed = vec!["libc-micro".to_owned()];
    let plan = resolve_dependencies("micro-editor", &available, &installed).expect("resolution failed");
    assert_eq!(plan.len(), 2);
    assert_eq!(plan[0].name, "libterm");
    assert_eq!(plan[1].name, "micro-editor");

    // 3. Missing dependency
    let mut p_broken = PackageMeta::new("broken-pkg", "1.0.0");
    p_broken.depends = vec!["nonexistent-dep".to_owned()];
    let broken_avail = vec![p_broken];
    let res = resolve_dependencies("broken-pkg", &broken_avail, &[]);
    assert_eq!(
        res,
        Err(DepError::MissingDependency {
            package: "broken-pkg".to_owned(),
            depends_on: "nonexistent-dep".to_owned(),
        })
    );

    // 4. Circular dependency (A depends on B, B depends on A)
    let mut p_a = PackageMeta::new("pkg-a", "1.0.0");
    p_a.depends = vec!["pkg-b".to_owned()];
    let mut p_b = PackageMeta::new("pkg-b", "1.0.0");
    p_b.depends = vec!["pkg-a".to_owned()];
    let circular_avail = vec![p_a, p_b];
    let res = resolve_dependencies("pkg-a", &circular_avail, &[]);
    assert_eq!(res, Err(DepError::CircularDependency("pkg-a".to_owned())));
}

fn test_dpkg_status_and_unpacking() {
    // 1. Initialize MicroFS in VFS using RamDisk
    let disk = Box::new(RamDisk::new(64 * 1024));
    {
        let mut vfs = VFS.lock();
        vfs.set_device(disk);
        vfs.format().expect("format failed");
    }

    // 2. Build sample package
    let meta = PackageMeta::new("demo-tool", "1.0.0");
    let control = "Package: demo-tool\nVersion: 1.0.0\nArchitecture: x86_64\nDescription: Demo Tool\n";
    let bin_data = b"DEMO_EXECUTABLE_BYTES";
    let deb_bytes = build_deb(control, &[("bin/demo", bin_data)]);
    let deb = parse_deb(&deb_bytes).expect("parse deb");

    // 3. Install via DpkgDatabase
    let mut db = DpkgDatabase::new();
    db.install(&deb, &meta).expect("install failed");

    assert!(db.is_installed("demo-tool"));
    let installed_rec = db.find("demo-tool").expect("package not found in db");
    assert_eq!(installed_rec.version, "1.0.0");
    assert_eq!(installed_rec.files, vec!["/bin/demo".to_owned()]);

    // 4. Verify file was unpacked into VFS
    let read_res = VFS.lock().read_file("/bin/demo").expect("read file failed");
    assert_eq!(read_res, bin_data);

    // 5. Verify status file serialization
    let status_bytes = VFS.lock().read_file("/var/lib/dpkg/status").expect("read status failed");
    let status_str = core::str::from_utf8(&status_bytes).expect("utf8 status");
    let parsed_db = parse_status_file(status_str);
    assert_eq!(parsed_db.len(), 1);
    assert_eq!(parsed_db[0].name, "demo-tool");
    assert_eq!(parsed_db[0].files, vec!["/bin/demo".to_owned()]);

    // 6. Remove package and verify file deletion
    db.remove("demo-tool").expect("remove failed");
    assert!(!db.is_installed("demo-tool"));
    assert!(VFS.lock().read_file("/bin/demo").is_err(), "file should be deleted");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
