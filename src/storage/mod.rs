pub mod ata;
pub mod nvme;

use alloc::vec;
use alloc::vec::Vec;

pub trait BlockDevice: Send + Sync {
    /// Return sector size in bytes (typically 512).
    fn sector_size(&self) -> usize {
        512
    }

    /// Total number of available sectors.
    fn sector_count(&self) -> u64;

    /// Read one sector (512 bytes) from the given LBA into `buf`.
    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), &'static str>;

    /// Write one sector (512 bytes) from `buf` to the given LBA.
    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), &'static str>;

    /// Read multiple contiguous sectors.
    fn read_sectors(&mut self, start_lba: u64, count: usize, buf: &mut [u8]) -> Result<(), &'static str> {
        let sec_sz = self.sector_size();
        if buf.len() < count * sec_sz {
            return Err("Buffer too small for requested sectors");
        }
        for i in 0..count {
            let lba = start_lba + i as u64;
            let offset = i * sec_sz;
            self.read_sector(lba, &mut buf[offset..offset + sec_sz])?;
        }
        Ok(())
    }

    /// Write multiple contiguous sectors.
    fn write_sectors(&mut self, start_lba: u64, count: usize, buf: &[u8]) -> Result<(), &'static str> {
        let sec_sz = self.sector_size();
        if buf.len() < count * sec_sz {
            return Err("Buffer too small for requested sectors");
        }
        for i in 0..count {
            let lba = start_lba + i as u64;
            let offset = i * sec_sz;
            self.write_sector(lba, &buf[offset..offset + sec_sz])?;
        }
        Ok(())
    }
}

/// A RAM-backed block device useful for memory file systems or hermetic unit testing.
pub struct RamDisk {
    data: Vec<u8>,
    sector_size: usize,
}

impl RamDisk {
    pub fn new(total_bytes: usize) -> Self {
        RamDisk {
            data: vec![0u8; total_bytes],
            sector_size: 512,
        }
    }
}

impl BlockDevice for RamDisk {
    fn sector_size(&self) -> usize {
        self.sector_size
    }

    fn sector_count(&self) -> u64 {
        (self.data.len() / self.sector_size) as u64
    }

    fn read_sector(&mut self, lba: u64, buf: &mut [u8]) -> Result<(), &'static str> {
        let offset = (lba as usize) * self.sector_size;
        if offset + self.sector_size > self.data.len() || buf.len() < self.sector_size {
            return Err("RamDisk read out of bounds");
        }
        buf[..self.sector_size].copy_from_slice(&self.data[offset..offset + self.sector_size]);
        Ok(())
    }

    fn write_sector(&mut self, lba: u64, buf: &[u8]) -> Result<(), &'static str> {
        let offset = (lba as usize) * self.sector_size;
        if offset + self.sector_size > self.data.len() || buf.len() < self.sector_size {
            return Err("RamDisk write out of bounds");
        }
        self.data[offset..offset + self.sector_size].copy_from_slice(&buf[..self.sector_size]);
        Ok(())
    }
}
