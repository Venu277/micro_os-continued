use super::BlockDevice;
use x86_64::instructions::port::{Port, PortReadOnly, PortWriteOnly};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaBus {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaDriveSelect {
    Master = 0xA0,
    Slave = 0xB0,
}

pub struct AtaDrive {
    data_port: Port<u16>,
    #[allow(dead_code)]
    error_port: PortReadOnly<u8>,
    sector_count_port: Port<u8>,
    lba_lo_port: Port<u8>,
    lba_mid_port: Port<u8>,
    lba_hi_port: Port<u8>,
    drive_select_port: Port<u8>,
    status_port: PortReadOnly<u8>,
    command_port: PortWriteOnly<u8>,
    #[allow(dead_code)]
    control_port: PortWriteOnly<u8>,
    drive_select: AtaDriveSelect,
    sector_count: u64,
}

impl AtaDrive {
    pub fn new(bus: AtaBus, drive_select: AtaDriveSelect) -> Result<Self, &'static str> {
        let base_port = match bus {
            AtaBus::Primary => 0x1F0,
            AtaBus::Secondary => 0x170,
        };
        let control_port = match bus {
            AtaBus::Primary => 0x3F6,
            AtaBus::Secondary => 0x376,
        };

        let mut drive = AtaDrive {
            data_port: Port::new(base_port),
            error_port: PortReadOnly::new(base_port + 1),
            sector_count_port: Port::new(base_port + 2),
            lba_lo_port: Port::new(base_port + 3),
            lba_mid_port: Port::new(base_port + 4),
            lba_hi_port: Port::new(base_port + 5),
            drive_select_port: Port::new(base_port + 6),
            status_port: PortReadOnly::new(base_port + 7),
            command_port: PortWriteOnly::new(base_port + 7),
            control_port: PortWriteOnly::new(control_port),
            drive_select,
            sector_count: 0,
        };

        // Disable ATA interrupts on this channel (nIEN bit 1 = 1) for polled PIO mode
        unsafe {
            drive.control_port.write(0x02);
        }

        drive.identify()?;
        Ok(drive)
    }

    /// Wait 400ns by reading the alternate status port 4 times (per ATA specification).
    fn wait_400ns(&mut self) {
        for _ in 0..4 {
            let _ = unsafe { self.status_port.read() };
        }
    }

    /// Poll until BSY is clear. Returns status byte.
    fn poll_not_busy(&mut self) -> Result<u8, &'static str> {
        self.wait_400ns();
        for _ in 0..100_000 {
            let status = unsafe { self.status_port.read() };
            if (status & 0x80) == 0 {
                return Ok(status);
            }
            core::hint::spin_loop();
        }
        Err("ATA device timeout waiting for BSY to clear")
    }

    /// Poll until DRQ is set and BSY is clear.
    fn poll_drq(&mut self) -> Result<(), &'static str> {
        self.wait_400ns();
        for _ in 0..100_000 {
            let status = unsafe { self.status_port.read() };
            if (status & 0x01) != 0 {
                return Err("ATA device returned error status (ERR set)");
            }
            if (status & 0x20) != 0 {
                return Err("ATA device fault (DF set)");
            }
            if (status & 0x80) == 0 && (status & 0x08) != 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err("ATA device timeout waiting for DRQ")
    }

    /// Probe and query disk identification data using ATA IDENTIFY (0xEC).
    pub fn identify(&mut self) -> Result<(), &'static str> {
        unsafe {
            // Ensure interrupts are disabled (nIEN = 1)
            self.control_port.write(0x02);

            // Select drive
            self.drive_select_port.write(self.drive_select as u8);
            self.wait_400ns();

            // Set sector count and LBA ports to 0
            self.sector_count_port.write(0);
            self.lba_lo_port.write(0);
            self.lba_mid_port.write(0);
            self.lba_hi_port.write(0);

            // Send IDENTIFY command
            self.command_port.write(0xEC);
            self.wait_400ns();

            let status = self.status_port.read();
            if status == 0 || status == 0xFF {
                return Err("No ATA drive detected on specified port");
            }

            self.poll_drq()?;

            // Read 256 words (512 bytes) of IDENTIFY data
            let mut identify_data = [0u16; 256];
            for word in identify_data.iter_mut() {
                *word = self.data_port.read();
            }

            // Word 60 and 61 represent 28-bit total addressable sectors
            let lba28_sectors = (identify_data[60] as u32) | ((identify_data[61] as u32) << 16);
            // Word 100-103 for 48-bit LBA (if supported)
            let lba48_sectors = (identify_data[100] as u64)
                | ((identify_data[101] as u64) << 16)
                | ((identify_data[102] as u64) << 32)
                | ((identify_data[103] as u64) << 48);

            self.sector_count = if lba48_sectors > 0 {
                lba48_sectors
            } else {
                lba28_sectors as u64
            };

            if self.sector_count == 0 {
                self.sector_count = 8192; // default 4 MiB virtual fallback
            }

            Ok(())
        }
    }
}

impl BlockDevice for AtaDrive {
    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        if buf.len() < 512 {
            return Err("Buffer must be at least 512 bytes");
        }

        unsafe {
            self.poll_not_busy()?;

            // Select drive and highest 4 bits of LBA28
            let drive_bits = match self.drive_select {
                AtaDriveSelect::Master => 0xE0,
                AtaDriveSelect::Slave => 0xF0,
            };
            self.drive_select_port
                .write(drive_bits | (((lba >> 24) & 0x0F) as u8));
            self.wait_400ns();

            // Write sector count (1) and LBA low, mid, high
            self.sector_count_port.write(1);
            self.lba_lo_port.write((lba & 0xFF) as u8);
            self.lba_mid_port.write(((lba >> 8) & 0xFF) as u8);
            self.lba_hi_port.write(((lba >> 16) & 0xFF) as u8);

            // Send READ SECTORS (0x20)
            self.command_port.write(0x20);

            self.poll_drq()?;

            // Read 256 16-bit words (512 bytes)
            for i in 0..256 {
                let word = self.data_port.read();
                buf[i * 2] = (word & 0xFF) as u8;
                buf[i * 2 + 1] = ((word >> 8) & 0xFF) as u8;
            }

            Ok(())
        }
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), &'static str> {
        if buf.len() < 512 {
            return Err("Buffer must be at least 512 bytes");
        }

        unsafe {
            self.poll_not_busy()?;

            let drive_bits = match self.drive_select {
                AtaDriveSelect::Master => 0xE0,
                AtaDriveSelect::Slave => 0xF0,
            };
            self.drive_select_port
                .write(drive_bits | (((lba >> 24) & 0x0F) as u8));
            self.wait_400ns();

            self.sector_count_port.write(1);
            self.lba_lo_port.write((lba & 0xFF) as u8);
            self.lba_mid_port.write(((lba >> 8) & 0xFF) as u8);
            self.lba_hi_port.write(((lba >> 16) & 0xFF) as u8);

            // Send WRITE SECTORS (0x30)
            self.command_port.write(0x30);

            self.poll_drq()?;

            // Write 256 16-bit words (512 bytes)
            for i in 0..256 {
                let word = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
                self.data_port.write(word);
            }

            // Flush cache
            self.command_port.write(0xE7);
            self.poll_not_busy()?;

            Ok(())
        }
    }
}
