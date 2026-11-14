use super::microfs::{DirEntry, FileType, MicroFs, ROOT_INODE_ID};
use crate::storage::BlockDevice;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

pub struct Vfs {
    device: Option<Box<dyn BlockDevice>>,
    pub current_dir_inode: u32,
    pub current_path: String,
}

impl Vfs {
    pub const fn new() -> Self {
        Vfs {
            device: None,
            current_dir_inode: ROOT_INODE_ID,
            current_path: String::new(),
        }
    }

    pub fn set_device(&mut self, dev: Box<dyn BlockDevice>) {
        self.device = Some(dev);
        self.current_dir_inode = ROOT_INODE_ID;
        self.current_path = String::from("/");
    }

    pub fn is_mounted(&self) -> bool {
        self.device.is_some()
    }

    pub fn dev_mut(&mut self) -> Option<&mut (dyn BlockDevice + 'static)> {
        self.device.as_deref_mut()
    }

    fn dev(&mut self) -> Result<&mut (dyn BlockDevice + 'static), &'static str> {
        self.device.as_deref_mut().ok_or("No storage device mounted")
    }

    pub fn format(&mut self) -> Result<(), &'static str> {
        let dev = self.dev()?;
        MicroFs::format(dev)?;
        self.current_dir_inode = ROOT_INODE_ID;
        self.current_path = String::from("/");
        Ok(())
    }

    /// Resolve a path (e.g. "/", "docs", "/docs/sub", "..") to inode ID.
    pub fn resolve_path(&mut self, path: &str) -> Result<u32, &'static str> {
        if path.is_empty() || path == "/" {
            return Ok(ROOT_INODE_ID);
        }

        let start_inode = if path.starts_with('/') {
            ROOT_INODE_ID
        } else {
            self.current_dir_inode
        };

        let dev = self.dev()?;
        let mut curr_inode_id = start_inode;

        for part in path.split('/').filter(|s| !s.is_empty()) {
            let inode = MicroFs::read_inode(dev, curr_inode_id)?;
            let entries = MicroFs::read_dir(dev, &inode)?;
            let found = entries
                .iter()
                .find(|e| e.name_as_str() == part)
                .ok_or("Path not found")?;

            curr_inode_id = found.inode;
        }

        Ok(curr_inode_id)
    }

    pub fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, &'static str> {
        let inode_id = if path.is_empty() {
            self.current_dir_inode
        } else {
            self.resolve_path(path)?
        };
        let dev = self.dev()?;
        let inode = MicroFs::read_inode(dev, inode_id)?;
        MicroFs::read_dir(dev, &inode)
    }

    pub fn change_dir(&mut self, path: &str) -> Result<(), &'static str> {
        let inode_id = self.resolve_path(path)?;
        let dev = self.dev()?;
        let inode = MicroFs::read_inode(dev, inode_id)?;
        if inode.file_type != FileType::Directory as u8 {
            return Err("Target is not a directory");
        }

        self.current_dir_inode = inode_id;

        // Update current_path string
        if path == "/" {
            self.current_path = String::from("/");
        } else if path == ".." {
            if self.current_path != "/" {
                if let Some(pos) = self.current_path.rfind('/') {
                    if pos == 0 {
                        self.current_path = String::from("/");
                    } else {
                        self.current_path.truncate(pos);
                    }
                }
            }
        } else if path == "." {
            // Keep unchanged
        } else if path.starts_with('/') {
            self.current_path = String::from(path);
        } else {
            if self.current_path == "/" {
                self.current_path.push_str(path);
            } else {
                self.current_path.push('/');
                self.current_path.push_str(path);
            }
        }

        Ok(())
    }

    fn split_parent_and_name(path: &str) -> (&str, &str) {
        let clean = path.trim_end_matches('/');
        if clean.is_empty() {
            return ("/", "");
        }
        if let Some(pos) = clean.rfind('/') {
            let parent = if pos == 0 { "/" } else { &clean[..pos] };
            let name = &clean[pos + 1..];
            (parent, name)
        } else {
            ("", clean)
        }
    }

    pub fn make_dir_p(&mut self, path: &str) -> Result<(), &'static str> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Ok(());
        }

        let mut curr_inode = if path.starts_with('/') {
            ROOT_INODE_ID
        } else {
            self.current_dir_inode
        };

        for part in parts {
            let dev = self.dev()?;
            let inode = MicroFs::read_inode(dev, curr_inode)?;
            let entries = MicroFs::read_dir(dev, &inode)?;
            if let Some(existing) = entries.iter().find(|e| e.name_as_str() == part) {
                if existing.file_type != FileType::Directory as u8 {
                    return Err("Path component is not a directory");
                }
                curr_inode = existing.inode;
            } else {
                let new_inode_id = MicroFs::alloc_inode(dev, FileType::Directory)?;
                let data_block = MicroFs::alloc_block(dev)?;
                let dot = DirEntry::new(new_inode_id, FileType::Directory, ".");
                let dotdot = DirEntry::new(curr_inode, FileType::Directory, "..");
                let mut dir_buf = [0u8; 512];
                unsafe {
                    let ptr_dot = &dot as *const DirEntry as *const u8;
                    dir_buf[0..64].copy_from_slice(core::slice::from_raw_parts(ptr_dot, 64));
                    let ptr_dotdot = &dotdot as *const DirEntry as *const u8;
                    dir_buf[64..128].copy_from_slice(core::slice::from_raw_parts(ptr_dotdot, 64));
                }
                dev.write_sector(data_block as u64, &dir_buf)?;

                let mut in_node = MicroFs::read_inode(dev, new_inode_id)?;
                in_node.size_bytes = 128;
                in_node.direct_blocks[0] = data_block;
                MicroFs::write_inode(dev, new_inode_id, &in_node)?;

                MicroFs::add_dir_entry(dev, curr_inode, part, new_inode_id, FileType::Directory)?;
                curr_inode = new_inode_id;
            }
        }
        Ok(())
    }

    pub fn make_dir(&mut self, name: &str) -> Result<(), &'static str> {
        self.make_dir_p(name)
    }

    pub fn touch(&mut self, name: &str) -> Result<(), &'static str> {
        let (parent_path, file_name) = Self::split_parent_and_name(name);
        if file_name.is_empty() {
            return Err("Invalid file name");
        }
        let parent_dir = if parent_path.is_empty() {
            self.current_dir_inode
        } else {
            self.make_dir_p(parent_path)?;
            self.resolve_path(parent_path)?
        };
        let dev = self.dev()?;
        let new_inode_id = MicroFs::alloc_inode(dev, FileType::Regular)?;
        MicroFs::add_dir_entry(dev, parent_dir, file_name, new_inode_id, FileType::Regular)
    }

    pub fn write_file(&mut self, name: &str, data: &[u8]) -> Result<usize, &'static str> {
        let target_inode = match self.resolve_path(name) {
            Ok(id) => id,
            Err(_) => {
                let (parent_path, file_name) = Self::split_parent_and_name(name);
                if file_name.is_empty() {
                    return Err("Invalid file name");
                }
                let parent_dir = if parent_path.is_empty() {
                    self.current_dir_inode
                } else {
                    self.make_dir_p(parent_path)?;
                    self.resolve_path(parent_path)?
                };
                let dev = self.dev()?;
                let new_inode_id = MicroFs::alloc_inode(dev, FileType::Regular)?;
                MicroFs::add_dir_entry(dev, parent_dir, file_name, new_inode_id, FileType::Regular)?;
                new_inode_id
            }
        };

        let dev = self.dev()?;
        MicroFs::write_file(dev, target_inode, data)
    }

    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>, &'static str> {
        let inode_id = self.resolve_path(path)?;
        let dev = self.dev()?;
        let inode = MicroFs::read_inode(dev, inode_id)?;
        if inode.file_type != FileType::Regular as u8 {
            return Err("Target is not a regular file");
        }

        let mut buf = alloc::vec![0u8; inode.size_bytes as usize];
        MicroFs::read_file(dev, &inode, 0, &mut buf)?;
        Ok(buf)
    }

    pub fn remove(&mut self, name: &str) -> Result<(), &'static str> {
        let (parent_path, file_name) = Self::split_parent_and_name(name);
        if file_name.is_empty() {
            return Err("Invalid path");
        }
        let parent_dir = if parent_path.is_empty() {
            self.current_dir_inode
        } else {
            self.resolve_path(parent_path)?
        };
        let dev = self.dev()?;
        MicroFs::remove_entry(dev, parent_dir, file_name)
    }

    pub fn disk_usage(&mut self) -> Result<(u32, u32, u32, u32, u32), &'static str> {
        let dev = self.dev()?;
        let sb = MicroFs::read_superblock(dev)?;
        let total_kb = (sb.total_blocks * 512) / 1024;
        let free_kb = (sb.free_blocks * 512) / 1024;
        let used_kb = total_kb.saturating_sub(free_kb);
        Ok((total_kb, used_kb, free_kb, sb.total_inodes, sb.free_inodes))
    }
}

pub static VFS: Mutex<Vfs> = Mutex::new(Vfs::new());
