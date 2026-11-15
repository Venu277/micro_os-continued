//! MicroOS Snap Container Image Format (`.snap`)
//!
//! Implements a self-contained read-only package container with a magic header,
//! table of contents (TOC), and file payload blocks.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

use crate::snap::manifest::SnapManifest;

pub const SNAP_MAGIC: &[u8; 8] = b"SNAP\x01\x00\x00\x00";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapFileEntry {
    pub path: String,
    pub is_dir: bool,
    pub permissions: u16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapContainer {
    pub name: String,
    pub version: String,
    pub revision: u32,
    pub files: Vec<SnapFileEntry>,
}

impl SnapContainer {
    pub fn new(name: &str, version: &str, revision: u32) -> Self {
        SnapContainer {
            name: name.to_owned(),
            version: version.to_owned(),
            revision,
            files: Vec::new(),
        }
    }

    /// Extract and parse `meta/snap.yaml` from this container.
    pub fn extract_manifest(&self) -> Result<SnapManifest, &'static str> {
        let entry = self
            .files
            .iter()
            .find(|f| f.path == "meta/snap.yaml" || f.path == "/meta/snap.yaml")
            .ok_or("Container missing 'meta/snap.yaml'")?;

        let text = core::str::from_utf8(&entry.data).map_err(|_| "Invalid UTF-8 in snap.yaml")?;
        SnapManifest::parse(text)
    }

    /// Find a file by path in the container.
    pub fn get_file(&self, path: &str) -> Option<&SnapFileEntry> {
        let clean = path.trim_start_matches('/');
        self.files.iter().find(|f| f.path.trim_start_matches('/') == clean)
    }
}

/// Parse a `.snap` binary container byte slice.
pub fn parse_snap(bytes: &[u8]) -> Result<SnapContainer, &'static str> {
    if bytes.len() < 64 {
        return Err("Snap image too small for header");
    }

    // 1. Verify Magic
    if &bytes[0..8] != SNAP_MAGIC {
        return Err("Invalid snap magic signature");
    }

    // 2. Header parsing
    let name_slice = &bytes[8..40];
    let name_len = name_slice.iter().position(|&b| b == 0).unwrap_or(32);
    let name = core::str::from_utf8(&name_slice[..name_len]).map_err(|_| "Invalid snap name UTF-8")?;

    let ver_slice = &bytes[40..56];
    let ver_len = ver_slice.iter().position(|&b| b == 0).unwrap_or(16);
    let version = core::str::from_utf8(&ver_slice[..ver_len]).map_err(|_| "Invalid snap version UTF-8")?;

    let revision = u32::from_le_bytes([bytes[56], bytes[57], bytes[58], bytes[59]]);
    let file_count = u32::from_le_bytes([bytes[60], bytes[61], bytes[62], bytes[63]]) as usize;

    let toc_offset = 64;
    let entry_header_size = 80; // 64 (path) + 1 (is_dir) + 2 (perm) + 5 (pad) + 4 (offset) + 4 (size)
    let toc_size = file_count * entry_header_size;
    let payload_start = toc_offset + toc_size;

    if bytes.len() < payload_start {
        return Err("Snap image truncated: TOC exceeds image size");
    }

    let mut files = Vec::with_capacity(file_count);

    for i in 0..file_count {
        let off = toc_offset + i * entry_header_size;
        let entry_slice = &bytes[off..off + entry_header_size];

        let path_slice = &entry_slice[0..64];
        let path_len = path_slice.iter().position(|&b| b == 0).unwrap_or(64);
        let path = core::str::from_utf8(&path_slice[..path_len]).map_err(|_| "Invalid file path in TOC")?;

        let is_dir = entry_slice[64] != 0;
        let permissions = u16::from_le_bytes([entry_slice[65], entry_slice[66]]);
        let file_off = u32::from_le_bytes([entry_slice[72], entry_slice[73], entry_slice[74], entry_slice[75]]) as usize;
        let file_size = u32::from_le_bytes([entry_slice[76], entry_slice[77], entry_slice[78], entry_slice[79]]) as usize;

        let abs_start = payload_start + file_off;
        let abs_end = abs_start + file_size;

        if abs_end > bytes.len() {
            return Err("Snap payload truncated: file extends past EOF");
        }

        let data = bytes[abs_start..abs_end].to_vec();

        files.push(SnapFileEntry {
            path: path.to_owned(),
            is_dir,
            permissions,
            data,
        });
    }

    Ok(SnapContainer {
        name: name.to_owned(),
        version: version.to_owned(),
        revision,
        files,
    })
}

/// Build a `.snap` binary container byte buffer.
pub fn build_snap(
    name: &str,
    version: &str,
    revision: u32,
    files: &[(&str, bool, u16, &[u8])],
) -> Vec<u8> {
    let mut out = Vec::new();

    // 1. Magic
    out.extend_from_slice(SNAP_MAGIC);

    // 2. Name (32 bytes)
    let mut name_buf = [0u8; 32];
    let n_bytes = name.as_bytes();
    let n_len = n_bytes.len().min(31);
    name_buf[..n_len].copy_from_slice(&n_bytes[..n_len]);
    out.extend_from_slice(&name_buf);

    // 3. Version (16 bytes)
    let mut ver_buf = [0u8; 16];
    let v_bytes = version.as_bytes();
    let v_len = v_bytes.len().min(15);
    ver_buf[..v_len].copy_from_slice(&v_bytes[..v_len]);
    out.extend_from_slice(&ver_buf);

    // 4. Revision (4 bytes)
    out.extend_from_slice(&revision.to_le_bytes());

    // 5. File Count (4 bytes)
    let file_count = files.len() as u32;
    out.extend_from_slice(&file_count.to_le_bytes());

    // 6. Build TOC & accumulate payload
    let mut payload = Vec::new();

    for &(path, is_dir, perm, data) in files {
        let mut entry_buf = [0u8; 80];

        // Path (64 bytes)
        let p_bytes = path.as_bytes();
        let p_len = p_bytes.len().min(63);
        entry_buf[..p_len].copy_from_slice(&p_bytes[..p_len]);

        // is_dir (1 byte)
        entry_buf[64] = if is_dir { 1 } else { 0 };

        // permissions (2 bytes)
        entry_buf[65..67].copy_from_slice(&perm.to_le_bytes());

        // pad 67..72 are 0

        // offset (4 bytes relative to payload start)
        let cur_off = payload.len() as u32;
        entry_buf[72..76].copy_from_slice(&cur_off.to_le_bytes());

        // size (4 bytes)
        let cur_size = data.len() as u32;
        entry_buf[76..80].copy_from_slice(&cur_size.to_le_bytes());

        out.extend_from_slice(&entry_buf);
        payload.extend_from_slice(data);
    }

    // 7. Append payload
    out.extend_from_slice(&payload);
    out
}
