//! MicroOS Debian `.deb` Archive Parser & Builder
//!
//! A real `.deb` package is a Unix `ar` archive containing three members:
//! 1. `debian-binary`: Text file containing format version ("2.0\n").
//! 2. `control.tar`: Tar archive containing the package `control` file.
//! 3. `data.tar`: Tar archive containing the payload files to install.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

pub const AR_MAGIC: &[u8; 8] = b"!<arch>\n";

/// An entry in a Unix `ar` archive.
#[derive(Debug, Clone)]
pub struct ArMember {
    pub name: String,
    pub size: usize,
    pub data: Vec<u8>,
}

/// An extracted file entry from a `tar` payload.
#[derive(Debug, Clone)]
pub struct TarFile {
    pub path: String,
    pub is_dir: bool,
    pub data: Vec<u8>,
}

/// Parsed Debian package contents.
#[derive(Debug, Clone)]
pub struct DebPackage {
    pub version: String,
    pub control_text: String,
    pub files: Vec<TarFile>,
}

/// Parse a raw Debian `.deb` binary slice.
pub fn parse_deb(bytes: &[u8]) -> Result<DebPackage, &'static str> {
    if bytes.len() < 8 || &bytes[..8] != AR_MAGIC {
        return Err("Not a valid Unix ar archive (missing !<arch>\\n magic)");
    }

    let members = parse_ar(&bytes[8..])?;

    // 1. Validate debian-binary
    let deb_bin = members
        .iter()
        .find(|m| m.name.starts_with("debian-binary"))
        .ok_or("Missing debian-binary member in .deb archive")?;
    let ver_str = core::str::from_utf8(&deb_bin.data)
        .map_err(|_| "Malformed debian-binary content")?
        .trim();
    if !ver_str.starts_with("2.0") {
        return Err("Unsupported Debian package version (expected 2.0)");
    }

    // 2. Parse control.tar
    let control_member = members
        .iter()
        .find(|m| m.name.starts_with("control.tar"))
        .ok_or("Missing control.tar member in .deb archive")?;
    let control_files = parse_tar(&control_member.data)?;
    let control_file = control_files
        .iter()
        .find(|f| f.path == "control" || f.path == "./control")
        .ok_or("Missing 'control' file inside control.tar")?;
    let control_text = core::str::from_utf8(&control_file.data)
        .map_err(|_| "Invalid UTF-8 in control file")?
        .to_owned();

    // 3. Parse data.tar
    let data_member = members
        .iter()
        .find(|m| m.name.starts_with("data.tar"))
        .ok_or("Missing data.tar member in .deb archive")?;
    let files = parse_tar(&data_member.data)?;

    Ok(DebPackage {
        version: ver_str.to_owned(),
        control_text,
        files,
    })
}

/// Parse Unix `ar` archive members.
pub fn parse_ar(mut bytes: &[u8]) -> Result<Vec<ArMember>, &'static str> {
    let mut members = Vec::new();

    while bytes.len() >= 60 {
        let name_raw = core::str::from_utf8(&bytes[0..16]).map_err(|_| "Invalid ar header name")?;
        let name = name_raw.trim().trim_end_matches('/').to_owned();

        let size_raw = core::str::from_utf8(&bytes[48..58]).map_err(|_| "Invalid ar header size")?;
        let size: usize = size_raw.trim().parse().map_err(|_| "Failed to parse ar member size")?;

        if &bytes[58..60] != b"`\n" {
            return Err("Invalid ar header trailer marker");
        }

        bytes = &bytes[60..];
        if bytes.len() < size {
            return Err("Unexpected EOF reading ar member data");
        }

        let data = bytes[..size].to_vec();
        members.push(ArMember { name, size, data });

        // Skip to 2-byte alignment
        let skip = if size % 2 == 1 { size + 1 } else { size };
        if bytes.len() < skip {
            break;
        }
        bytes = &bytes[skip..];
    }

    Ok(members)
}

/// Parse uncompressed USTAR `tar` archive payload.
pub fn parse_tar(mut bytes: &[u8]) -> Result<Vec<TarFile>, &'static str> {
    let mut files = Vec::new();

    while bytes.len() >= 512 {
        // Two consecutive zero blocks signal end of archive
        if bytes[..512].iter().all(|&b| b == 0) {
            break;
        }

        let name_raw = core::str::from_utf8(&bytes[0..100]).map_err(|_| "Invalid tar file name")?;
        let raw_name = name_raw.trim_matches('\0').trim();
        if raw_name.is_empty() {
            break;
        }

        // Clean leading "./" or "/"
        let clean_name = raw_name.trim_start_matches('.').trim_start_matches('/');

        let size_raw = core::str::from_utf8(&bytes[124..136]).map_err(|_| "Invalid tar file size")?;
        let size_str = size_raw.trim_matches('\0').trim();
        let size = if size_str.is_empty() {
            0
        } else {
            usize::from_str_radix(size_str, 8).map_err(|_| "Failed to parse octal tar size")?
        };

        let typeflag = bytes[156];
        let is_dir = typeflag == b'5' || raw_name.ends_with('/');

        bytes = &bytes[512..];
        if bytes.len() < size {
            return Err("Unexpected EOF reading tar file payload");
        }

        let data = bytes[..size].to_vec();
        files.push(TarFile {
            path: clean_name.to_owned(),
            is_dir,
            data,
        });

        // 512-byte block alignment
        let remainder = size % 512;
        let skip = if remainder != 0 { size + (512 - remainder) } else { size };
        if bytes.len() < skip {
            break;
        }
        bytes = &bytes[skip..];
    }

    Ok(files)
}

/// Helper to construct a valid `.deb` byte buffer in memory.
pub fn build_deb(control_content: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut deb = Vec::new();
    deb.extend_from_slice(AR_MAGIC);

    // 1. debian-binary
    let deb_bin = b"2.0\n";
    append_ar_member(&mut deb, "debian-binary", deb_bin);

    // 2. control.tar
    let mut control_tar = Vec::new();
    append_tar_file(&mut control_tar, "control", control_content.as_bytes(), false);
    append_tar_eof(&mut control_tar);
    append_ar_member(&mut deb, "control.tar", &control_tar);

    // 3. data.tar
    let mut data_tar = Vec::new();
    for (path, content) in files {
        append_tar_file(&mut data_tar, path, content, false);
    }
    append_tar_eof(&mut data_tar);
    append_ar_member(&mut deb, "data.tar", &data_tar);

    deb
}

fn append_ar_member(buf: &mut Vec<u8>, name: &str, data: &[u8]) {
    let mut hdr = [b' '; 60];
    let name_str = format!("{}/", name);
    hdr[..name_str.len().min(16)].copy_from_slice(&name_str.as_bytes()[..name_str.len().min(16)]);

    // Timestamp
    hdr[16..28].copy_from_slice(b"1700000000  ");
    // Owner & Group
    hdr[28..34].copy_from_slice(b"0     ");
    hdr[34..40].copy_from_slice(b"0     ");
    // Mode
    hdr[40..48].copy_from_slice(b"100644  ");
    // Size
    let size_str = format!("{:<10}", data.len());
    hdr[48..58].copy_from_slice(size_str.as_bytes());
    // Trailer
    hdr[58..60].copy_from_slice(b"`\n");

    buf.extend_from_slice(&hdr);
    buf.extend_from_slice(data);
    if data.len() % 2 == 1 {
        buf.push(b'\n'); // 2-byte pad
    }
}

fn append_tar_file(buf: &mut Vec<u8>, name: &str, data: &[u8], is_dir: bool) {
    let mut hdr = [0u8; 512];
    let name_bytes = name.as_bytes();
    hdr[..name_bytes.len().min(100)].copy_from_slice(&name_bytes[..name_bytes.len().min(100)]);

    // Mode
    let mode = if is_dir { b"0000755\0" } else { b"0000644\0" };
    hdr[100..108].copy_from_slice(mode);

    // UID / GID
    hdr[108..116].copy_from_slice(b"0000000\0");
    hdr[116..124].copy_from_slice(b"0000000\0");

    // Size (octal)
    let size_octal = format!("{:011o}\0", if is_dir { 0 } else { data.len() });
    hdr[124..136].copy_from_slice(size_octal.as_bytes());

    // Mtime
    hdr[136..148].copy_from_slice(b"14000000000\0");

    // Typeflag
    hdr[156] = if is_dir { b'5' } else { b'0' };

    // Magic: ustar\0
    hdr[257..263].copy_from_slice(b"ustar\0");
    hdr[263..265].copy_from_slice(b"00");

    // Checksum: calculate with chksum field filled with spaces (8 spaces)
    hdr[148..156].fill(b' ');
    let sum: u32 = hdr.iter().map(|&b| b as u32).sum();
    let chksum_str = format!("{:06o}\0 ", sum);
    hdr[148..156].copy_from_slice(chksum_str.as_bytes());

    buf.extend_from_slice(&hdr);
    if !is_dir && !data.is_empty() {
        buf.extend_from_slice(data);
        let rem = data.len() % 512;
        if rem != 0 {
            buf.extend_from_slice(&vec![0u8; 512 - rem]);
        }
    }
}

fn append_tar_eof(buf: &mut Vec<u8>) {
    buf.extend_from_slice(&[0u8; 1024]);
}
