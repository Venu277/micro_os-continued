#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use micro_os::fs::microfs::MicroFs;
use micro_os::fs::vfs::Vfs;
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

    serial_print!("filesystem::format...\t");
    test_format();
    serial_println!("[ok]");

    serial_print!("filesystem::create_and_read...\t");
    test_create_and_read();
    serial_println!("[ok]");

    serial_print!("filesystem::directory_tree...\t");
    test_directory_tree();
    serial_println!("[ok]");

    serial_print!("filesystem::large_file_indirect...\t");
    test_large_file();
    serial_println!("[ok]");

    serial_print!("filesystem::reboot_persistence...\t");
    test_reboot_persistence();
    serial_println!("[ok]");

    exit_qemu(QemuExitCode::Success);
    micro_os::hlt_loop();
}

fn test_format() {
    let mut disk = RamDisk::new(64 * 1024);
    MicroFs::format(&mut disk).expect("format failed");
    let sb = MicroFs::read_superblock(&mut disk).expect("read superblock failed");
    assert_eq!(sb.magic, micro_os::fs::microfs::MAGIC);
    assert_eq!(sb.block_size, 512);
}

fn test_create_and_read() {
    let disk = Box::new(RamDisk::new(64 * 1024));
    let mut vfs = Vfs::new();
    vfs.set_device(disk);
    vfs.format().expect("format failed");

    let content = b"MicroOS Persistent Filesystem Verification String";
    vfs.write_file("test.txt", content).expect("write failed");

    let read_back = vfs.read_file("test.txt").expect("read failed");
    assert_eq!(&read_back[..], content);
}

fn test_directory_tree() {
    let disk = Box::new(RamDisk::new(64 * 1024));
    let mut vfs = Vfs::new();
    vfs.set_device(disk);
    vfs.format().expect("format failed");

    vfs.make_dir("system").expect("mkdir system failed");
    vfs.make_dir("user").expect("mkdir user failed");

    vfs.change_dir("system").expect("cd system failed");
    vfs.write_file("os.conf", b"kernel_ver=0.5").expect("write os.conf failed");

    vfs.make_dir("drivers").expect("mkdir drivers failed");
    vfs.change_dir("drivers").expect("cd drivers failed");
    vfs.write_file("ata.drv", b"ata_pio_lba28").expect("write ata.drv failed");

    // Test .. navigation
    vfs.change_dir("..").expect("cd .. failed");
    let entries = vfs.list_dir("").expect("ls failed");
    assert!(entries.iter().any(|e| e.name_as_str() == "drivers"));
    assert!(entries.iter().any(|e| e.name_as_str() == "os.conf"));

    // Verify absolute path read
    let driver_file = vfs.read_file("/system/drivers/ata.drv").expect("read nested failed");
    assert_eq!(&driver_file[..], b"ata_pio_lba28");
}

fn test_large_file() {
    let disk = Box::new(RamDisk::new(64 * 1024));
    let mut vfs = Vfs::new();
    vfs.set_device(disk);
    vfs.format().expect("format failed");

    // 8 KiB file requires 16 sectors (10 direct + 6 indirect sectors)
    let size = 8 * 1024;
    let mut large_data = vec![0u8; size];
    for (i, b) in large_data.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }

    vfs.write_file("large.bin", &large_data).expect("large write failed");
    let read_data = vfs.read_file("large.bin").expect("large read failed");
    assert_eq!(read_data.len(), size);
    assert_eq!(read_data, large_data);
}

fn test_reboot_persistence() {
    // 1. Simulate Boot Session 1: Write file and dismount
    let mut backing_store = RamDisk::new(64 * 1024);
    MicroFs::format(&mut backing_store).expect("format failed");

    let secret = b"PERSISTENT_DATA_ACROSS_REBOOT_SESSION_12345";
    let inode_id = MicroFs::alloc_inode(&mut backing_store, micro_os::fs::microfs::FileType::Regular)
        .expect("alloc inode failed");
    MicroFs::add_dir_entry(
        &mut backing_store,
        micro_os::fs::microfs::ROOT_INODE_ID,
        "persist.dat",
        inode_id,
        micro_os::fs::microfs::FileType::Regular,
    )
    .expect("add dir entry failed");
    MicroFs::write_file(&mut backing_store, inode_id, secret).expect("write failed");

    // 2. Simulate Reboot: Mount fresh VFS on the same raw disk bytes
    let disk = Box::new(backing_store);
    let mut vfs_reboot = Vfs::new();
    vfs_reboot.set_device(disk);

    // 3. Read back data in the second "boot"
    let entries = vfs_reboot.list_dir("").expect("ls on reboot failed");
    assert!(entries.iter().any(|e| e.name_as_str() == "persist.dat"));

    let data = vfs_reboot.read_file("persist.dat").expect("read on reboot failed");
    assert_eq!(&data[..], secret);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    micro_os::test_panic_handler(info)
}
