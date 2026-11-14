use crate::storage::BlockDevice;
use alloc::vec::Vec;
use core::mem;

pub const MAGIC: u32 = 0x4D494352; // "MICR"
pub const BLOCK_SIZE: usize = 512;
pub const SUPERBLOCK_LBA: u64 = 1;
pub const INODE_BITMAP_LBA: u64 = 2;
pub const BLOCK_BITMAP_START_LBA: u64 = 3;
pub const BLOCK_BITMAP_BLOCKS: usize = 2; // 2 * 512 bytes = 8192 bits
pub const INODE_TABLE_START_LBA: u64 = 5;
pub const INODE_TABLE_BLOCKS: usize = 32; // 32 * 512 = 16384 bytes = 256 inodes (64 bytes each)
pub const DATA_BLOCK_START_LBA: u64 = 37;
pub const ROOT_INODE_ID: u32 = 1;
pub const MAX_INODES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FileType {
    Unused = 0,
    Regular = 1,
    Directory = 2,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Superblock {
    pub magic: u32,
    pub version: u32,
    pub block_size: u32,
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub total_inodes: u32,
    pub free_inodes: u32,
    pub root_inode: u32,
    pub padding: [u8; 480],
}

impl Superblock {
    pub fn new(total_blocks: u32) -> Self {
        Superblock {
            magic: MAGIC,
            version: 1,
            block_size: BLOCK_SIZE as u32,
            total_blocks,
            free_blocks: total_blocks.saturating_sub(DATA_BLOCK_START_LBA as u32),
            total_inodes: MAX_INODES as u32,
            free_inodes: (MAX_INODES - 1) as u32, // root inode used
            root_inode: ROOT_INODE_ID,
            padding: [0u8; 480],
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Inode {
    pub file_type: u8,
    pub _reserved1: u8,
    pub permissions: u16,
    pub link_count: u16,
    pub _reserved2: u16,
    pub size_bytes: u32,
    pub direct_blocks: [u32; 10], // 10 * 512 = 5 KiB direct
    pub indirect_block: u32,      // Points to block containing 128 * 512 bytes = 64 KiB
    pub padding: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<Inode>() == 64);
const _: () = assert!(core::mem::size_of::<DirEntry>() == 64);
const _: () = assert!(core::mem::size_of::<Superblock>() == 512);

impl Inode {
    pub fn empty() -> Self {
        Inode {
            file_type: FileType::Unused as u8,
            _reserved1: 0,
            permissions: 0,
            link_count: 0,
            _reserved2: 0,
            size_bytes: 0,
            direct_blocks: [0; 10],
            indirect_block: 0,
            padding: [0; 8],
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DirEntry {
    pub inode: u32,
    pub file_type: u8,
    pub name_len: u8,
    pub name: [u8; 58],
}

impl DirEntry {
    pub fn empty() -> Self {
        DirEntry {
            inode: 0,
            file_type: 0,
            name_len: 0,
            name: [0u8; 58],
        }
    }

    pub fn new(inode: u32, file_type: FileType, name_str: &str) -> Self {
        let mut entry = DirEntry::empty();
        entry.inode = inode;
        entry.file_type = file_type as u8;
        let bytes = name_str.as_bytes();
        let len = bytes.len().min(57);
        entry.name_len = len as u8;
        entry.name[..len].copy_from_slice(&bytes[..len]);
        entry
    }

    pub fn name_as_str(&self) -> &str {
        let len = (self.name_len as usize).min(57);
        core::str::from_utf8(&self.name[..len]).unwrap_or("")
    }
}

pub struct MicroFs;

impl MicroFs {
    /// Format a block device with a clean MicroFS filesystem.
    pub fn format(dev: &mut dyn BlockDevice) -> Result<(), &'static str> {
        let total_blocks = dev.sector_count() as u32;
        if total_blocks < 100 {
            return Err("Disk too small for MicroFS");
        }

        // 1. Write Superblock
        let sb = Superblock::new(total_blocks);
        let mut sb_buf = [0u8; BLOCK_SIZE];
        unsafe {
            let ptr = &sb as *const Superblock as *const u8;
            sb_buf.copy_from_slice(core::slice::from_raw_parts(ptr, BLOCK_SIZE));
        }
        dev.write_sector(SUPERBLOCK_LBA, &sb_buf)?;

        // 2. Zero Inode Bitmap (mark Inode 0 and Inode 1 as used)
        let mut inode_bitmap = [0u8; BLOCK_SIZE];
        inode_bitmap[0] = 0b0000_0011; // Inode 0 reserved, Inode 1 (root) used
        dev.write_sector(INODE_BITMAP_LBA, &inode_bitmap)?;

        // 3. Block Bitmap (mark system blocks 0..37 as used, and root dir data block 37 as used)
        let mut block_bitmap = [0u8; BLOCK_SIZE * BLOCK_BITMAP_BLOCKS];
        // 38 blocks used (0..37)
        for i in 0..=37 {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            block_bitmap[byte_idx] |= 1 << bit_idx;
        }
        dev.write_sector(BLOCK_BITMAP_START_LBA, &block_bitmap[..BLOCK_SIZE])?;
        dev.write_sector(BLOCK_BITMAP_START_LBA + 1, &block_bitmap[BLOCK_SIZE..])?;

        // 4. Inode Table (Zero all, set Inode 1 = Root Directory)
        let zero_sector = [0u8; BLOCK_SIZE];
        for i in 0..INODE_TABLE_BLOCKS {
            dev.write_sector(INODE_TABLE_START_LBA + i as u64, &zero_sector)?;
        }

        let mut root_inode = Inode::empty();
        root_inode.file_type = FileType::Directory as u8;
        root_inode.permissions = 0o755;
        root_inode.link_count = 2; // '.' and parent
        root_inode.size_bytes = (2 * mem::size_of::<DirEntry>()) as u32;
        root_inode.direct_blocks[0] = DATA_BLOCK_START_LBA as u32;
        Self::write_inode(dev, ROOT_INODE_ID, &root_inode)?;

        // 5. Populate Root Directory with '.' and '..'
        let mut dir_buf = [0u8; BLOCK_SIZE];
        let dot = DirEntry::new(ROOT_INODE_ID, FileType::Directory, ".");
        let dotdot = DirEntry::new(ROOT_INODE_ID, FileType::Directory, "..");
        unsafe {
            let ptr_dot = &dot as *const DirEntry as *const u8;
            dir_buf[0..64].copy_from_slice(core::slice::from_raw_parts(ptr_dot, 64));
            let ptr_dotdot = &dotdot as *const DirEntry as *const u8;
            dir_buf[64..128].copy_from_slice(core::slice::from_raw_parts(ptr_dotdot, 64));
        }
        dev.write_sector(DATA_BLOCK_START_LBA, &dir_buf)?;

        Ok(())
    }

    /// Read the Superblock and verify filesystem validity.
    pub fn read_superblock(dev: &mut dyn BlockDevice) -> Result<Superblock, &'static str> {
        let mut buf = [0u8; BLOCK_SIZE];
        dev.read_sector(SUPERBLOCK_LBA, &mut buf)?;
        let sb: Superblock = unsafe { core::ptr::read(buf.as_ptr() as *const _) };
        if sb.magic != MAGIC {
            return Err("Invalid MicroFS magic identifier");
        }
        Ok(sb)
    }

    pub fn read_inode(dev: &mut dyn BlockDevice, inode_id: u32) -> Result<Inode, &'static str> {
        if inode_id == 0 || inode_id as usize > MAX_INODES {
            return Err("Invalid inode ID");
        }
        let index = inode_id - 1;
        let sector_offset = (index as usize * 64) / BLOCK_SIZE;
        let byte_offset = (index as usize * 64) % BLOCK_SIZE;

        let mut buf = [0u8; BLOCK_SIZE];
        dev.read_sector(INODE_TABLE_START_LBA + sector_offset as u64, &mut buf)?;
        let inode: Inode =
            unsafe { core::ptr::read(buf[byte_offset..].as_ptr() as *const Inode) };
        Ok(inode)
    }

    pub fn write_inode(
        dev: &mut dyn BlockDevice,
        inode_id: u32,
        inode: &Inode,
    ) -> Result<(), &'static str> {
        if inode_id == 0 || inode_id as usize > MAX_INODES {
            return Err("Invalid inode ID");
        }
        let index = inode_id - 1;
        let sector_offset = (index as usize * 64) / BLOCK_SIZE;
        let byte_offset = (index as usize * 64) % BLOCK_SIZE;

        let mut buf = [0u8; BLOCK_SIZE];
        let lba = INODE_TABLE_START_LBA + sector_offset as u64;
        dev.read_sector(lba, &mut buf)?;
        unsafe {
            let src = inode as *const Inode as *const u8;
            buf[byte_offset..byte_offset + 64]
                .copy_from_slice(core::slice::from_raw_parts(src, 64));
        }
        dev.write_sector(lba, &buf)?;
        Ok(())
    }

    /// Allocate a new inode.
    pub fn alloc_inode(dev: &mut dyn BlockDevice, file_type: FileType) -> Result<u32, &'static str> {
        let mut bitmap = [0u8; BLOCK_SIZE];
        dev.read_sector(INODE_BITMAP_LBA, &mut bitmap)?;

        for i in 1..MAX_INODES {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                // Free inode found
                bitmap[byte_idx] |= 1 << bit_idx;
                dev.write_sector(INODE_BITMAP_LBA, &bitmap)?;

                let mut inode = Inode::empty();
                inode.file_type = file_type as u8;
                inode.permissions = if file_type == FileType::Directory { 0o755 } else { 0o644 };
                inode.link_count = 1;
                Self::write_inode(dev, i as u32, &inode)?;
                return Ok(i as u32);
            }
        }
        Err("No free inodes available")
    }

    /// Free an inode.
    pub fn free_inode(dev: &mut dyn BlockDevice, inode_id: u32) -> Result<(), &'static str> {
        if inode_id <= 1 || inode_id as usize > MAX_INODES {
            return Err("Cannot free reserved or root inode");
        }
        let mut bitmap = [0u8; BLOCK_SIZE];
        dev.read_sector(INODE_BITMAP_LBA, &mut bitmap)?;
        let byte_idx = (inode_id as usize) / 8;
        let bit_idx = (inode_id as usize) % 8;
        bitmap[byte_idx] &= !(1 << bit_idx);
        dev.write_sector(INODE_BITMAP_LBA, &bitmap)?;

        let empty = Inode::empty();
        Self::write_inode(dev, inode_id, &empty)?;
        Ok(())
    }

    /// Allocate a new data block.
    pub fn alloc_block(dev: &mut dyn BlockDevice) -> Result<u32, &'static str> {
        let mut bitmap = [0u8; BLOCK_SIZE * BLOCK_BITMAP_BLOCKS];
        dev.read_sector(BLOCK_BITMAP_START_LBA, &mut bitmap[..BLOCK_SIZE])?;
        dev.read_sector(BLOCK_BITMAP_START_LBA + 1, &mut bitmap[BLOCK_SIZE..])?;

        let total_blocks = dev.sector_count() as usize;
        for i in DATA_BLOCK_START_LBA as usize..total_blocks {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            if (bitmap[byte_idx] & (1 << bit_idx)) == 0 {
                bitmap[byte_idx] |= 1 << bit_idx;
                dev.write_sector(BLOCK_BITMAP_START_LBA, &bitmap[..BLOCK_SIZE])?;
                dev.write_sector(BLOCK_BITMAP_START_LBA + 1, &bitmap[BLOCK_SIZE..])?;

                // Zero allocated block
                let zeros = [0u8; BLOCK_SIZE];
                dev.write_sector(i as u64, &zeros)?;
                return Ok(i as u32);
            }
        }
        Err("Disk full: no free data blocks")
    }

    /// Read directory entries for a directory inode.
    pub fn read_dir(dev: &mut dyn BlockDevice, inode: &Inode) -> Result<Vec<DirEntry>, &'static str> {
        if inode.file_type != FileType::Directory as u8 {
            return Err("Not a directory");
        }

        let mut entries = Vec::new();
        let total_entries = (inode.size_bytes as usize) / mem::size_of::<DirEntry>();
        let mut read_count = 0;

        for &block_id in inode.direct_blocks.iter() {
            if block_id == 0 || read_count >= total_entries {
                break;
            }
            let mut buf = [0u8; BLOCK_SIZE];
            dev.read_sector(block_id as u64, &mut buf)?;

            for chunk in buf.chunks_exact(64) {
                if read_count >= total_entries {
                    break;
                }
                let entry: DirEntry = unsafe { core::ptr::read(chunk.as_ptr() as *const DirEntry) };
                if entry.inode != 0 {
                    entries.push(entry);
                }
                read_count += 1;
            }
        }

        Ok(entries)
    }

    /// Add an entry to a directory.
    pub fn add_dir_entry(
        dev: &mut dyn BlockDevice,
        dir_inode_id: u32,
        entry_name: &str,
        child_inode_id: u32,
        child_type: FileType,
    ) -> Result<(), &'static str> {
        let mut dir_inode = Self::read_inode(dev, dir_inode_id)?;
        if dir_inode.file_type != FileType::Directory as u8 {
            return Err("Target is not a directory");
        }

        let new_entry = DirEntry::new(child_inode_id, child_type, entry_name);

        // Find an empty slot in existing direct blocks
        let entries_per_block = BLOCK_SIZE / 64;
        let mut placed = false;

        for block_idx in 0..dir_inode.direct_blocks.len() {
            let mut block_id = dir_inode.direct_blocks[block_idx];
            if block_id == 0 {
                // Need to allocate new data block
                block_id = Self::alloc_block(dev)?;
                dir_inode.direct_blocks[block_idx] = block_id;
            }

            let mut buf = [0u8; BLOCK_SIZE];
            dev.read_sector(block_id as u64, &mut buf)?;

            for i in 0..entries_per_block {
                let slot = &buf[i * 64..(i + 1) * 64];
                let existing_inode = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
                if existing_inode == 0 {
                    // Empty slot found!
                    unsafe {
                        let ptr = &new_entry as *const DirEntry as *const u8;
                        buf[i * 64..(i + 1) * 64].copy_from_slice(core::slice::from_raw_parts(ptr, 64));
                    }
                    dev.write_sector(block_id as u64, &buf)?;
                    dir_inode.size_bytes += 64;
                    Self::write_inode(dev, dir_inode_id, &dir_inode)?;
                    placed = true;
                    break;
                }
            }
            if placed {
                break;
            }
        }

        if !placed {
            return Err("Directory full");
        }

        Ok(())
    }

    /// Read file content.
    pub fn read_file(
        dev: &mut dyn BlockDevice,
        inode: &Inode,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<usize, &'static str> {
        let file_size = inode.size_bytes as usize;
        if offset >= file_size {
            return Ok(0);
        }

        let bytes_to_read = buf.len().min(file_size - offset);
        let mut bytes_read = 0;

        let mut current_offset = offset;
        while bytes_read < bytes_to_read {
            let block_index = current_offset / BLOCK_SIZE;
            let block_offset = current_offset % BLOCK_SIZE;
            let chunk_len = (BLOCK_SIZE - block_offset).min(bytes_to_read - bytes_read);

            let block_id = if block_index < 10 {
                inode.direct_blocks[block_index]
            } else {
                // Indirect block
                if inode.indirect_block == 0 {
                    break;
                }
                let mut ind_buf = [0u8; BLOCK_SIZE];
                dev.read_sector(inode.indirect_block as u64, &mut ind_buf)?;
                let ind_idx = block_index - 10;
                if ind_idx >= 128 {
                    break;
                }
                u32::from_le_bytes([
                    ind_buf[ind_idx * 4],
                    ind_buf[ind_idx * 4 + 1],
                    ind_buf[ind_idx * 4 + 2],
                    ind_buf[ind_idx * 4 + 3],
                ])
            };

            if block_id == 0 {
                break;
            }

            let mut sec_buf = [0u8; BLOCK_SIZE];
            dev.read_sector(block_id as u64, &mut sec_buf)?;
            buf[bytes_read..bytes_read + chunk_len]
                .copy_from_slice(&sec_buf[block_offset..block_offset + chunk_len]);

            bytes_read += chunk_len;
            current_offset += chunk_len;
        }

        Ok(bytes_read)
    }

    /// Write file content.
    pub fn write_file(
        dev: &mut dyn BlockDevice,
        inode_id: u32,
        data: &[u8],
    ) -> Result<usize, &'static str> {
        let mut inode = Self::read_inode(dev, inode_id)?;
        let mut bytes_written = 0;

        while bytes_written < data.len() {
            let block_index = bytes_written / BLOCK_SIZE;
            let block_offset = bytes_written % BLOCK_SIZE;
            let chunk_len = (BLOCK_SIZE - block_offset).min(data.len() - bytes_written);

            let block_id = if block_index < 10 {
                if inode.direct_blocks[block_index] == 0 {
                    let new_blk = Self::alloc_block(dev)?;
                    inode.direct_blocks[block_index] = new_blk;
                }
                inode.direct_blocks[block_index]
            } else {
                // Indirect block allocation
                if inode.indirect_block == 0 {
                    let new_ind = Self::alloc_block(dev)?;
                    inode.indirect_block = new_ind;
                }
                let mut ind_buf = [0u8; BLOCK_SIZE];
                dev.read_sector(inode.indirect_block as u64, &mut ind_buf)?;
                let ind_idx = block_index - 10;
                if ind_idx >= 128 {
                    return Err("File exceeds maximum supported size (69 KiB)");
                }
                let mut blk = u32::from_le_bytes([
                    ind_buf[ind_idx * 4],
                    ind_buf[ind_idx * 4 + 1],
                    ind_buf[ind_idx * 4 + 2],
                    ind_buf[ind_idx * 4 + 3],
                ]);
                if blk == 0 {
                    blk = Self::alloc_block(dev)?;
                    ind_buf[ind_idx * 4..ind_idx * 4 + 4].copy_from_slice(&blk.to_le_bytes());
                    dev.write_sector(inode.indirect_block as u64, &ind_buf)?;
                }
                blk
            };

            let mut sec_buf = [0u8; BLOCK_SIZE];
            if block_offset > 0 || chunk_len < BLOCK_SIZE {
                let _ = dev.read_sector(block_id as u64, &mut sec_buf);
            }
            sec_buf[block_offset..block_offset + chunk_len]
                .copy_from_slice(&data[bytes_written..bytes_written + chunk_len]);
            dev.write_sector(block_id as u64, &sec_buf)?;

            bytes_written += chunk_len;
        }

        inode.size_bytes = bytes_written as u32;
        Self::write_inode(dev, inode_id, &inode)?;
        Ok(bytes_written)
    }

    /// Remove a file or empty directory from its parent directory.
    pub fn remove_entry(
        dev: &mut dyn BlockDevice,
        dir_inode_id: u32,
        name: &str,
    ) -> Result<(), &'static str> {
        let mut dir_inode = Self::read_inode(dev, dir_inode_id)?;
        let entries = Self::read_dir(dev, &dir_inode)?;
        let target = entries
            .into_iter()
            .find(|e| e.name_as_str() == name)
            .ok_or("No such file or directory")?;

        // Clear entry in directory data blocks
        for &block_id in dir_inode.direct_blocks.iter() {
            if block_id == 0 {
                continue;
            }
            let mut buf = [0u8; BLOCK_SIZE];
            dev.read_sector(block_id as u64, &mut buf)?;
            let mut modified = false;

            for i in 0..(BLOCK_SIZE / 64) {
                let slot = &buf[i * 64..(i + 1) * 64];
                let entry_inode = u32::from_le_bytes([slot[0], slot[1], slot[2], slot[3]]);
                if entry_inode == target.inode {
                    // Zero entry
                    buf[i * 64..(i + 1) * 64].copy_from_slice(&[0u8; 64]);
                    modified = true;
                    break;
                }
            }

            if modified {
                dev.write_sector(block_id as u64, &buf)?;
                dir_inode.size_bytes = dir_inode.size_bytes.saturating_sub(64);
                Self::write_inode(dev, dir_inode_id, &dir_inode)?;
                break;
            }
        }

        // Free target inode
        Self::free_inode(dev, target.inode)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::RamDisk;

    #[test_case]
    fn test_microfs_structures() {
        assert_eq!(core::mem::size_of::<Superblock>(), 512);
        assert_eq!(core::mem::size_of::<Inode>(), 64);
        assert_eq!(core::mem::size_of::<DirEntry>(), 64);
    }

    #[test_case]
    fn test_superblock_magic() {
        let mut disk = RamDisk::new(64 * 1024);
        MicroFs::format(&mut disk).expect("format failed");
        let sb = MicroFs::read_superblock(&mut disk).expect("read superblock failed");
        assert_eq!(sb.magic, MAGIC);
        assert_eq!(sb.block_size, 512);
    }
}
