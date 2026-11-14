//! MicroOS dpkg Status Database & File Unpacker
//!
//! Manages on-disk package installation status at `/var/lib/dpkg/status` and
//! deploys `.deb` file payloads into the MicroOS Virtual File System.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::fs::vfs::VFS;
use crate::pkg::deb::DebPackage;
use crate::pkg::rfc822::PackageMeta;
use crate::serial_println;

pub const DPKG_STATUS_FILE: &str = "/var/lib/dpkg/status";

/// Represents an installed package recorded in `/var/lib/dpkg/status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub status: String, // "install ok installed"
    pub architecture: String,
    pub description: String,
    pub files: Vec<String>,
}

pub static DPKG_DB: Mutex<Option<DpkgDatabase>> = Mutex::new(None);

pub struct DpkgDatabase {
    pub installed: Vec<InstalledPackage>,
}

impl DpkgDatabase {
    pub fn new() -> Self {
        DpkgDatabase {
            installed: Vec::new(),
        }
    }

    /// Load or parse `/var/lib/dpkg/status` from disk.
    pub fn load(&mut self) {
        let mut vfs = VFS.lock();
        if let Ok(content_bytes) = vfs.read_file(DPKG_STATUS_FILE) {
            if let Ok(content) = core::str::from_utf8(&content_bytes) {
                self.installed = parse_status_file(content);
                serial_println!(
                    "dpkg: Loaded {} installed packages from '{}'",
                    self.installed.len(),
                    DPKG_STATUS_FILE
                );
                return;
            }
        }
        serial_println!("dpkg: No existing status database found; initialized empty database.");
    }

    /// Save installed packages into `/var/lib/dpkg/status`.
    pub fn save(&self) -> Result<(), &'static str> {
        let mut text = String::new();
        for pkg in &self.installed {
            text.push_str(&format!("Package: {}\n", pkg.name));
            text.push_str(&format!("Status: {}\n", pkg.status));
            text.push_str(&format!("Architecture: {}\n", pkg.architecture));
            text.push_str(&format!("Version: {}\n", pkg.version));
            text.push_str(&format!("Description: {}\n", pkg.description));
            if !pkg.files.is_empty() {
                text.push_str("Files:\n");
                for f in &pkg.files {
                    text.push_str(&format!(" {}\n", f));
                }
            }
            text.push('\n');
        }

        let mut vfs = VFS.lock();
        // Ensure /var/lib/dpkg directory exists
        let _ = vfs.make_dir("/var");
        let _ = vfs.make_dir("/var/lib");
        let _ = vfs.make_dir("/var/lib/dpkg");

        if vfs.write_file(DPKG_STATUS_FILE, text.as_bytes()).is_ok() {
            serial_println!("dpkg: Saved status database to '{}'", DPKG_STATUS_FILE);
            Ok(())
        } else {
            Err("Failed to write /var/lib/dpkg/status")
        }
    }

    /// Install a package: unpack payload to VFS and record in status DB.
    pub fn install(&mut self, deb: &DebPackage, meta: &PackageMeta) -> Result<(), &'static str> {
        let mut installed_files = Vec::new();

        {
            let mut vfs = VFS.lock();

            for file in &deb.files {
                let full_path = if file.path.starts_with('/') {
                    file.path.clone()
                } else {
                    format!("/{}", file.path)
                };

                if file.is_dir {
                    let _ = vfs.make_dir(&full_path);
                } else {
                    // Create parent directory if needed
                    if let Some(slash_idx) = full_path.rfind('/') {
                        let parent = &full_path[..slash_idx];
                        if !parent.is_empty() {
                            let _ = vfs.make_dir(parent);
                        }
                    }

                    if vfs.write_file(&full_path, &file.data).is_ok() {
                        serial_println!("dpkg: Unpacked file '{}' ({} bytes)", full_path, file.data.len());
                        installed_files.push(full_path);
                    }
                }
            }
        }

        // Update database entry
        self.installed.retain(|p| p.name != meta.name);
        self.installed.push(InstalledPackage {
            name: meta.name.clone(),
            version: meta.version.clone(),
            status: "install ok installed".to_owned(),
            architecture: meta.architecture.clone(),
            description: meta.description.clone(),
            files: installed_files,
        });

        let _ = self.save();
        Ok(())
    }

    /// Remove a package: delete unpacked files from VFS and remove from DB.
    pub fn remove(&mut self, name: &str) -> Result<(), &'static str> {
        let pkg_idx = self
            .installed
            .iter()
            .position(|p| p.name == name)
            .ok_or("Package is not installed")?;

        let pkg = self.installed.remove(pkg_idx);

        {
            let mut vfs = VFS.lock();
            for file_path in &pkg.files {
                let _ = vfs.remove(file_path);
                serial_println!("dpkg: Removed file '{}'", file_path);
            }
        }

        let _ = self.save();
        Ok(())
    }

    /// Check if package is installed.
    pub fn is_installed(&self, name: &str) -> bool {
        self.installed.iter().any(|p| p.name == name)
    }

    /// Find an installed package record.
    pub fn find(&self, name: &str) -> Option<&InstalledPackage> {
        self.installed.iter().find(|p| p.name == name)
    }
}

/// Parse RFC 822 format of `/var/lib/dpkg/status`.
pub fn parse_status_file(content: &str) -> Vec<InstalledPackage> {
    let mut packages = Vec::new();
    let paragraphs = content.split("\n\n");

    for p in paragraphs {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut name = String::new();
        let mut version = String::new();
        let mut status = "install ok installed".to_owned();
        let mut architecture = "x86_64".to_owned();
        let mut description = String::new();
        let mut files = Vec::new();
        let mut in_files = false;

        for line in trimmed.lines() {
            if line.starts_with("Files:") {
                in_files = true;
                continue;
            }

            if in_files {
                if line.starts_with(' ') || line.starts_with('\t') {
                    let f = line.trim();
                    if !f.is_empty() {
                        files.push(f.to_owned());
                    }
                    continue;
                } else {
                    in_files = false;
                }
            }

            if let Some(idx) = line.find(':') {
                let key = line[..idx].trim();
                let val = line[idx + 1..].trim();
                match key {
                    "Package" => name = val.to_owned(),
                    "Version" => version = val.to_owned(),
                    "Status" => status = val.to_owned(),
                    "Architecture" => architecture = val.to_owned(),
                    "Description" => description = val.to_owned(),
                    _ => {}
                }
            }
        }

        if !name.is_empty() {
            packages.push(InstalledPackage {
                name,
                version,
                status,
                architecture,
                description,
                files,
            });
        }
    }

    packages
}

/// Initialize the global DPKG database.
pub fn init_dpkg() {
    let mut db = DpkgDatabase::new();
    db.load();
    *DPKG_DB.lock() = Some(db);
}
