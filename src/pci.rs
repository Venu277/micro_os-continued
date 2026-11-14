use alloc::vec::Vec;
use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_id: u8,
    pub subclass_id: u8,
    pub prog_if: u8,
}

impl PciDevice {
    pub fn class_name(&self) -> &'static str {
        match self.class_id {
            0x00 => "Unclassified",
            0x01 => match self.subclass_id {
                0x01 => "IDE Controller",
                0x06 => "SATA / AHCI Controller",
                0x08 => "NVMe Controller",
                _ => "Mass Storage Controller",
            },
            0x02 => match self.subclass_id {
                0x00 => "Ethernet Controller",
                0x80 => "Network Controller",
                _ => "Network Controller",
            },
            0x03 => "Display Controller",
            0x06 => "Bridge Device (Host/PCI/ISA)",
            _ => "Other Peripheral",
        }
    }
}

pub fn pci_read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x8000_0000;

    unsafe {
        let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
        let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
        addr_port.write(address);
        data_port.read()
    }
}

pub fn pci_write_u32(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let address = ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC)
        | 0x8000_0000;

    unsafe {
        let mut addr_port: Port<u32> = Port::new(CONFIG_ADDRESS);
        let mut data_port: Port<u32> = Port::new(CONFIG_DATA);
        addr_port.write(address);
        data_port.write(value);
    }
}

pub fn enumerate_pci() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    for bus in 0..=31 {
        for device in 0..32 {
            let vendor_and_device = pci_read_u32(bus, device, 0, 0x00);
            let vendor_id = (vendor_and_device & 0xFFFF) as u16;
            if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                continue;
            }

            let device_id = ((vendor_and_device >> 16) & 0xFFFF) as u16;
            let class_rev = pci_read_u32(bus, device, 0, 0x08);
            let class_id = ((class_rev >> 24) & 0xFF) as u8;
            let subclass_id = ((class_rev >> 16) & 0xFF) as u8;
            let prog_if = ((class_rev >> 8) & 0xFF) as u8;

            devices.push(PciDevice {
                bus,
                device,
                function: 0,
                vendor_id,
                device_id,
                class_id,
                subclass_id,
                prog_if,
            });
        }
    }

    devices
}

impl PciDevice {
    pub fn read_bar(&self, bar_idx: u8) -> u32 {
        if bar_idx > 5 {
            return 0;
        }
        pci_read_u32(self.bus, self.device, self.function, 0x10 + bar_idx * 4)
    }

    pub fn enable_bus_mastering(&self) {
        let cmd = pci_read_u32(self.bus, self.device, self.function, 0x04);
        // Bit 0: I/O Space, Bit 1: Memory Space, Bit 2: Bus Master
        pci_write_u32(self.bus, self.device, self.function, 0x04, cmd | 0x07);
    }
}

pub fn find_e1000() -> Option<PciDevice> {
    enumerate_pci().into_iter().find(|dev| {
        dev.vendor_id == 0x8086
            && matches!(
                dev.device_id,
                0x100E // 82540EM (QEMU default)
                    | 0x1004 // 82543GC
                    | 0x100F // 82545EM (VMware Workstation default)
                    | 0x1079 // 82546GB
                    | 0x107C // 82541PI
            )
    })
}
