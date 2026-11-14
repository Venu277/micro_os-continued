use alloc::vec::Vec;
use spin::Mutex;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTableFlags, PhysFrame, Size4KiB, Translate,
};
use x86_64::{PhysAddr, VirtAddr};
use crate::pci::PciDevice;

// Register Offsets
pub const REG_CTRL: usize = 0x0000;
pub const REG_STATUS: usize = 0x0008;
pub const REG_EEPROM: usize = 0x0014;
pub const REG_ICR: usize = 0x00C0;
pub const REG_IMS: usize = 0x00D0;
pub const REG_IMC: usize = 0x00D8;
pub const REG_RCTL: usize = 0x0100;
pub const REG_TCTL: usize = 0x0400;
pub const REG_TIPG: usize = 0x0410;
pub const REG_RDBAL: usize = 0x2800;
pub const REG_RDBAH: usize = 0x2804;
pub const REG_RDLEN: usize = 0x2808;
pub const REG_RDH: usize = 0x2810;
pub const REG_RDT: usize = 0x2818;
pub const REG_TDBAL: usize = 0x3800;
pub const REG_TDBAH: usize = 0x3804;
pub const REG_TDLEN: usize = 0x3808;
pub const REG_TDH: usize = 0x3810;
pub const REG_TDT: usize = 0x3818;
pub const REG_MTA: usize = 0x5200;
pub const REG_RAL: usize = 0x5400;
pub const REG_RAH: usize = 0x5404;

pub const NUM_RX_DESC: usize = 32;
pub const NUM_TX_DESC: usize = 16;
pub const BUFFER_SIZE: usize = 2048;

pub const E1000_MMIO_VIRT_BASE: u64 = 0x_3333_0000_0000;

#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct RxDesc {
    pub addr: u64,
    pub length: u16,
    pub checksum: u16,
    pub status: u8,
    pub errors: u8,
    pub special: u16,
}

#[derive(Clone, Copy, Default)]
#[repr(C, align(16))]
pub struct TxDesc {
    pub addr: u64,
    pub length: u16,
    pub cso: u8,
    pub cmd: u8,
    pub status: u8,
    pub css: u8,
    pub special: u16,
}

#[repr(C, align(4096))]
struct DmaRing {
    rx_descs: [RxDesc; NUM_RX_DESC],
    tx_descs: [TxDesc; NUM_TX_DESC],
    rx_buffers: [[u8; BUFFER_SIZE]; NUM_RX_DESC],
    tx_buffers: [[u8; BUFFER_SIZE]; NUM_TX_DESC],
}

static mut DMA_RING: DmaRing = DmaRing {
    rx_descs: [RxDesc {
        addr: 0,
        length: 0,
        checksum: 0,
        status: 0,
        errors: 0,
        special: 0,
    }; NUM_RX_DESC],
    tx_descs: [TxDesc {
        addr: 0,
        length: 0,
        cso: 0,
        cmd: 0,
        status: 0,
        css: 0,
        special: 0,
    }; NUM_TX_DESC],
    rx_buffers: [[0u8; BUFFER_SIZE]; NUM_RX_DESC],
    tx_buffers: [[0u8; BUFFER_SIZE]; NUM_TX_DESC],
};

pub struct E1000Driver {
    mmio_base: u64,
    mac_address: [u8; 6],
    cur_rx: usize,
    cur_tx: usize,
    packets_rx: u64,
    packets_tx: u64,
    bytes_rx: u64,
    bytes_tx: u64,
}

impl E1000Driver {
    pub const fn empty() -> Self {
        E1000Driver {
            mmio_base: 0,
            mac_address: [0; 6],
            cur_rx: 0,
            cur_tx: 0,
            packets_rx: 0,
            packets_tx: 0,
            bytes_rx: 0,
            bytes_tx: 0,
        }
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac_address
    }

    pub fn stats(&self) -> (u64, u64, u64, u64) {
        (self.packets_rx, self.bytes_rx, self.packets_tx, self.bytes_tx)
    }

    unsafe fn read_reg(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.mmio_base + offset as u64) as *const u32)
    }

    unsafe fn write_reg(&self, offset: usize, value: u32) {
        core::ptr::write_volatile((self.mmio_base + offset as u64) as *mut u32, value);
    }

    pub fn init(
        &mut self,
        pci_dev: &PciDevice,
        mapper: &mut (impl Mapper<Size4KiB> + Translate),
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<(), &'static str> {
        // 1. Enable Bus Mastering and Memory Space in PCI command register
        pci_dev.enable_bus_mastering();

        // 2. Read MMIO physical address from BAR 0
        let bar0 = pci_dev.read_bar(0);
        let mmio_phys = (bar0 & 0xFFFF_FFF0) as u64;
        if mmio_phys == 0 {
            return Err("e1000 invalid BAR 0 MMIO address");
        }

        // 3. Map 128 KiB (32 pages of 4096 bytes) to virtual memory
        self.mmio_base = E1000_MMIO_VIRT_BASE;
        for i in 0..32 {
            let page = Page::containing_address(VirtAddr::new(self.mmio_base + (i * 4096) as u64));
            let frame = PhysFrame::containing_address(PhysAddr::new(mmio_phys + (i * 4096) as u64));
            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE;
            unsafe {
                if let Ok(mapping) = mapper.map_to(page, frame, flags, frame_allocator) {
                    mapping.flush();
                }
            }
        }

        // 4. Translate DMA descriptor rings and buffers to physical addresses
        unsafe {
            let ring_ptr = &raw mut DMA_RING;
            let rx_desc_virt = VirtAddr::new(&raw const (*ring_ptr).rx_descs as u64);
            let tx_desc_virt = VirtAddr::new(&raw const (*ring_ptr).tx_descs as u64);

            let rx_desc_phys = mapper
                .translate_addr(rx_desc_virt)
                .ok_or("Failed to translate Rx descriptor physical address")?;
            let tx_desc_phys = mapper
                .translate_addr(tx_desc_virt)
                .ok_or("Failed to translate Tx descriptor physical address")?;

            crate::serial_println!(
                "e1000: rx_desc_phys={:?}, tx_desc_phys={:?}",
                rx_desc_phys,
                tx_desc_phys
            );

            // Populate Rx buffer addresses
            for i in 0..NUM_RX_DESC {
                let buf_virt = VirtAddr::new(&raw const (*ring_ptr).rx_buffers[i] as u64);
                let buf_phys = mapper
                    .translate_addr(buf_virt)
                    .ok_or("Failed to translate Rx buffer address")?;
                (*ring_ptr).rx_descs[i].addr = buf_phys.as_u64();
                (*ring_ptr).rx_descs[i].status = 0;
            }

            // Populate Tx buffer addresses
            for i in 0..NUM_TX_DESC {
                let buf_virt = VirtAddr::new(&raw const (*ring_ptr).tx_buffers[i] as u64);
                let buf_phys = mapper
                    .translate_addr(buf_virt)
                    .ok_or("Failed to translate Tx buffer address")?;
                (*ring_ptr).tx_descs[i].addr = buf_phys.as_u64();
                (*ring_ptr).tx_descs[i].status = 0;
                (*ring_ptr).tx_descs[i].cmd = 0;
            }

            // 5. Reset device first
            let ctrl = self.read_reg(REG_CTRL);
            self.write_reg(REG_CTRL, ctrl | (1 << 26)); // RST
            let mut timeout = 100_000;
            while (self.read_reg(REG_CTRL) & (1 << 26)) != 0 && timeout > 0 {
                timeout -= 1;
                core::hint::spin_loop();
            }

            // Set Link Up (SLU bit 6) and auto-speed detection
            self.write_reg(REG_CTRL, (1 << 6) | (1 << 5));

            // 6. Read MAC address from EEPROM or RAL
            // In QEMU/VMware, EEPROM contains the factory MAC address
            let mut mac_found = false;
            for word in 0..3 {
                self.write_reg(REG_EEPROM, 1 | ((word as u32) << 8));
                let mut eeprom_timeout = 100_000;
                while (self.read_reg(REG_EEPROM) & (1 << 4)) == 0 && eeprom_timeout > 0 {
                    eeprom_timeout -= 1;
                    core::hint::spin_loop();
                }
                if eeprom_timeout > 0 {
                    let data = (self.read_reg(REG_EEPROM) >> 16) as u16;
                    self.mac_address[word * 2] = (data & 0xFF) as u8;
                    self.mac_address[word * 2 + 1] = ((data >> 8) & 0xFF) as u8;
                    mac_found = true;
                }
            }

            if !mac_found || self.mac_address == [0; 6] {
                let ral = self.read_reg(REG_RAL);
                let rah = self.read_reg(REG_RAH);
                self.mac_address[0] = (ral & 0xFF) as u8;
                self.mac_address[1] = ((ral >> 8) & 0xFF) as u8;
                self.mac_address[2] = ((ral >> 16) & 0xFF) as u8;
                self.mac_address[3] = ((ral >> 24) & 0xFF) as u8;
                self.mac_address[4] = (rah & 0xFF) as u8;
                self.mac_address[5] = ((rah >> 8) & 0xFF) as u8;
            }

            // Fallback default QEMU MAC if somehow unread
            if self.mac_address == [0; 6] {
                self.mac_address = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
            }

            // Write MAC into Receive Address registers with AV (Address Valid) bit 31 set
            let ral_val = u32::from_le_bytes([
                self.mac_address[0],
                self.mac_address[1],
                self.mac_address[2],
                self.mac_address[3],
            ]);
            let rah_val = (u32::from_le_bytes([self.mac_address[4], self.mac_address[5], 0, 0]))
                | (1 << 31);
            self.write_reg(REG_RAL, ral_val);
            self.write_reg(REG_RAH, rah_val);

            // 7. Clear Multicast Table Array
            for i in 0..128 {
                self.write_reg(REG_MTA + i * 4, 0);
            }

            // 8. Disable all interrupts for polled operation
            self.write_reg(REG_IMC, 0xFFFF_FFFF);
            let _ = self.read_reg(REG_ICR);

            // 9. Configure Receive Ring
            self.write_reg(REG_RDBAL, rx_desc_phys.as_u64() as u32);
            self.write_reg(REG_RDBAH, (rx_desc_phys.as_u64() >> 32) as u32);
            self.write_reg(REG_RDLEN, (NUM_RX_DESC * 16) as u32);
            self.write_reg(REG_RDH, 0);
            self.write_reg(REG_RDT, (NUM_RX_DESC - 1) as u32);

            // RCTL: EN(1<<1), UPE(1<<3), MPE(1<<4), BAM(1<<15), SECRC(1<<26)
            self.write_reg(
                REG_RCTL,
                (1 << 1) | (1 << 3) | (1 << 4) | (1 << 15) | (1 << 26),
            );

            // 10. Configure Transmit Ring
            self.write_reg(REG_TDBAL, tx_desc_phys.as_u64() as u32);
            self.write_reg(REG_TDBAH, (tx_desc_phys.as_u64() >> 32) as u32);
            self.write_reg(REG_TDLEN, (NUM_TX_DESC * 16) as u32);
            self.write_reg(REG_TDH, 0);
            self.write_reg(REG_TDT, 0);

            // Configure TIPG (Inter Packet Gap) for standard IEEE 802.3
            self.write_reg(REG_TIPG, 10 | (8 << 10) | (6 << 20));

            // TCTL: EN(1<<1), PSP(1<<3), CT=0x0F, COLD=0x40
            self.write_reg(REG_TCTL, (1 << 1) | (1 << 3) | (0x0F << 4) | (0x40 << 12));

            self.cur_rx = 0;
            self.cur_tx = 0;
        }

        Ok(())
    }

    /// Transmit raw Ethernet frame across the wire.
    pub fn transmit(&mut self, data: &[u8]) -> bool {
        if data.len() > BUFFER_SIZE {
            return false;
        }

        unsafe {
            let ring = &raw mut DMA_RING;
            let tx_idx = self.cur_tx;

            // Copy data into TX buffer
            (&mut (*ring).tx_buffers[tx_idx])[..data.len()].copy_from_slice(data);

            // Set descriptor
            (*ring).tx_descs[tx_idx].length = data.len() as u16;
            // EOP (End of Packet: 1 << 0) | IFCS (Insert FCS: 1 << 1) | RS (Report Status: 1 << 3)
            (*ring).tx_descs[tx_idx].cmd = 0x0B;
            (*ring).tx_descs[tx_idx].status = 0;

            // Advance tail pointer
            self.cur_tx = (self.cur_tx + 1) % NUM_TX_DESC;
            self.write_reg(REG_TDT, self.cur_tx as u32);

            // Wait until descriptor is processed
            let mut timeout = 100_000;
            while ((*ring).tx_descs[tx_idx].status & 0x01) == 0 && timeout > 0 {
                timeout -= 1;
                core::hint::spin_loop();
            }

            if timeout == 0 {
                crate::serial_println!("e1000: transmit descriptor TIMEOUT (status={})", (*ring).tx_descs[tx_idx].status);
            } else {
                crate::serial_println!("e1000: transmitted {} bytes (status={})", data.len(), (*ring).tx_descs[tx_idx].status);
            }

            self.packets_tx += 1;
            self.bytes_tx += data.len() as u64;

            true
        }
    }

    /// Receive pending raw Ethernet frame if available.
    pub fn receive(&mut self) -> Option<Vec<u8>> {
        unsafe {
            let ring = &raw mut DMA_RING;
            let rx_idx = self.cur_rx;

            // Check DD (Descriptor Done) bit 0
            if ((*ring).rx_descs[rx_idx].status & 0x01) == 0 {
                return None;
            }

            let len = (*ring).rx_descs[rx_idx].length as usize;
            crate::serial_println!("e1000: received packet len={}", len);
            let mut packet = Vec::with_capacity(len);
            packet.extend_from_slice(&(&(*ring).rx_buffers[rx_idx])[..len]);

            // Clear status
            (*ring).rx_descs[rx_idx].status = 0;

            // Advance tail
            self.write_reg(REG_RDT, rx_idx as u32);
            self.cur_rx = (self.cur_rx + 1) % NUM_RX_DESC;

            self.packets_rx += 1;
            self.bytes_rx += len as u64;

            Some(packet)
        }
    }
}

pub static E1000_DRIVER: Mutex<E1000Driver> = Mutex::new(E1000Driver::empty());
