#![no_std]
#![no_main]

extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;

use micro_os::fs::vfs::VFS;
use micro_os::snap::container::{build_snap, parse_snap};
use micro_os::snap::manifest::{Confinement, SnapManifest};
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

    serial_print!("snap_store::manifest_parser...\t");
    test_snap_manifest_parse_and_serialize();
    serial_println!("[ok]");

    serial_print!("snap_store::container_pack_unpack...\t");
    test_snap_container_build_and_parse();
    serial_println!("[ok]");

    serial_print!("snap_store::install_and_sandbox_run...\t");
    test_snap_store_install_and_run();
    serial_println!("[ok]");

    serial_print!("snap_store::revision_upgrade_and_revert...\t");
    test_snap_revision_upgrade_and_revert();
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_snap_manifest_parse_and_serialize() {
    let sample_yaml = "\
name: micro-calc
version: 1.4.0
summary: Lightweight calculator
description: Fast terminal calculator
confinement: strict
grade: stable
apps:
  micro-calc:
    command: bin/calc
    plugs:
      - network
plugs:
  - network
";

    let manifest = SnapManifest::parse(sample_yaml).expect("parse manifest failed");
    assert_eq!(manifest.name, "micro-calc");
    assert_eq!(manifest.version, "1.4.0");
    assert_eq!(manifest.summary, "Lightweight calculator");
    assert_eq!(manifest.confinement, Confinement::Strict);
    assert_eq!(manifest.apps.len(), 1);
    assert_eq!(manifest.apps[0].name, "micro-calc");
    assert_eq!(manifest.apps[0].command, "bin/calc");
    assert_eq!(manifest.apps[0].plugs, vec!["network".to_owned()]);
    assert_eq!(manifest.plugs, vec!["network".to_owned()]);

    let serialized = manifest.to_yaml();
    let re_parsed = SnapManifest::parse(&serialized).expect("re-parse serialized yaml");
    assert_eq!(manifest, re_parsed);
}

fn test_snap_container_build_and_parse() {
    let manifest_yaml = "\
name: test-snap
version: 2.0.0
summary: Test snap container
description: Container test
confinement: classic
grade: stable
";

    let bin_data = b"EXECUTABLE_BYTECODE_12345";
    let doc_data = b"Readme documentation text";

    let files = [
        ("meta/snap.yaml", false, 0o644, manifest_yaml.as_bytes()),
        ("bin/testapp", false, 0o755, bin_data.as_slice()),
        ("doc/README", false, 0o644, doc_data.as_slice()),
    ];

    let snap_bytes = build_snap("test-snap", "2.0.0", 42, &files);
    assert!(snap_bytes.len() > 64);

    let container = parse_snap(&snap_bytes).expect("parse container failed");
    assert_eq!(container.name, "test-snap");
    assert_eq!(container.version, "2.0.0");
    assert_eq!(container.revision, 42);
    assert_eq!(container.files.len(), 3);

    let app_file = container.get_file("bin/testapp").expect("find bin/testapp");
    assert_eq!(app_file.data, bin_data);
    assert_eq!(app_file.permissions, 0o755);

    let manifest = container.extract_manifest().expect("extract manifest");
    assert_eq!(manifest.name, "test-snap");
    assert_eq!(manifest.version, "2.0.0");
    assert_eq!(manifest.confinement, Confinement::Classic);
}

fn test_snap_store_install_and_run() {
    // 1. Initialize MicroFS in VFS using RamDisk
    let disk = Box::new(RamDisk::new(64 * 1024));
    {
        let mut vfs = VFS.lock();
        vfs.set_device(disk);
        vfs.format().expect("format failed");
    }

    // 2. Initialize Snap Store
    init_snap();

    // 3. Search store catalog
    {
        let lock = SNAP_MANAGER.lock();
        let mgr = lock.as_ref().unwrap();
        let results = mgr.find("calc");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "micro-calc");
        assert_eq!(results[0].revision, 3);
    }

    // 4. Install micro-calc
    {
        let mut lock = SNAP_MANAGER.lock();
        let mgr = lock.as_mut().unwrap();
        mgr.install("micro-calc", None, false).expect("install micro-calc");
        assert_eq!(mgr.list().len(), 1);
        let s = &mgr.list()[0];
        assert_eq!(s.name, "micro-calc");
        assert_eq!(s.active_revision, 3);
        assert_eq!(s.connections.len(), 1);
        assert_eq!(s.connections[0].plug, "micro-calc:network");
    }

    // 5. Verify filesystem mount layout
    {
        let mut vfs = VFS.lock();
        let bin_bytes = vfs.read_file("/snap/micro-calc/3/bin/calc").expect("read mounted bin");
        assert_eq!(bin_bytes, b"CALC_BINARY_EXECUTABLE_BYTES");

        let current_target = vfs.read_file("/snap/micro-calc/current").expect("read current");
        assert_eq!(core::str::from_utf8(&current_target).unwrap(), "/snap/micro-calc/3");

        let wrapper = vfs.read_file("/snap/bin/micro-calc").expect("read wrapper");
        assert!(core::str::from_utf8(&wrapper).unwrap().contains("SNAP=/snap/micro-calc/3"));
    }

    // 6. Execute sandboxed app
    {
        let lock = SNAP_MANAGER.lock();
        let mgr = lock.as_ref().unwrap();
        let run_out = mgr.run("micro-calc", &["15", "+", "27"]).expect("run micro-calc");
        assert!(run_out.contains("15 + 27 = 42"));
        assert!(run_out.contains("confinement=strict"));
    }
}

fn test_snap_revision_upgrade_and_revert() {
    // 1. Initialize MicroFS in VFS
    let disk = Box::new(RamDisk::new(64 * 1024));
    {
        let mut vfs = VFS.lock();
        vfs.set_device(disk);
        vfs.format().expect("format failed");
    }

    // 2. Initialize Snap Store with fresh manager
    init_snap();

    // 3. Install revision 1
    {
        let mut lock = SNAP_MANAGER.lock();
        let mgr = lock.as_mut().unwrap();

        // Mutate store catalog micro-calc to rev 1
        if let Some(pkg) = mgr.store_catalog.iter_mut().find(|p| p.name == "micro-calc") {
            pkg.revision = 1;
            pkg.version = "1.0.0".to_owned();
        }
        mgr.install("micro-calc", None, false).expect("install rev 1");
        assert_eq!(mgr.list()[0].active_revision, 1);
    }

    // 4. Update store catalog to revision 2 and install (upgrade)
    {
        let mut lock = SNAP_MANAGER.lock();
        let mgr = lock.as_mut().unwrap();

        if let Some(pkg) = mgr.store_catalog.iter_mut().find(|p| p.name == "micro-calc") {
            pkg.revision = 2;
            pkg.version = "1.1.0".to_owned();
        }
        mgr.install("micro-calc", None, false).expect("install rev 2");
        assert_eq!(mgr.list()[0].active_revision, 2);
        assert_eq!(mgr.list()[0].revisions.len(), 2);
    }

    // 5. Revert back to revision 1
    {
        let mut lock = SNAP_MANAGER.lock();
        let mgr = lock.as_mut().unwrap();
        let reverted_to = mgr.revert("micro-calc").expect("revert");
        assert_eq!(reverted_to, 1);
        assert_eq!(mgr.list()[0].active_revision, 1);
    }

    // 6. Verify filesystem current pointer reflects reverted revision 1
    {
        let mut vfs = VFS.lock();
        let current_target = vfs.read_file("/snap/micro-calc/current").expect("read current");
        assert_eq!(core::str::from_utf8(&current_target).unwrap(), "/snap/micro-calc/1");
    }

    // 7. Remove snap and verify purge
    {
        let mut lock = SNAP_MANAGER.lock();
        let mgr = lock.as_mut().unwrap();
        mgr.remove("micro-calc").expect("remove micro-calc");
        assert!(mgr.list().is_empty());
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
