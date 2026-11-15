//! MicroOS Snap Runtime Environment & Loopback Mount Simulation
//!
//! Manages mounted revision directories under `/snap/<name>/<rev>`,
//! `/snap/<name>/current` aliases, wrapper scripts in `/snap/bin/`, and
//! security interface plug/slot connections.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::vfs::VFS;
use crate::serial_println;
use crate::snap::container::SnapContainer;
use crate::snap::manifest::SnapManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceConnection {
    pub plug: String,       // e.g. "micro-calc:network"
    pub slot: String,       // e.g. ":network"
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRevision {
    pub revision: u32,
    pub version: String,
    pub mount_path: String,
    pub manifest: SnapManifest,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledSnap {
    pub name: String,
    pub active_revision: u32,
    pub channel: String,
    pub publisher: String,
    pub revisions: Vec<InstalledRevision>,
    pub connections: Vec<InterfaceConnection>,
}

impl InstalledSnap {
    pub fn new(name: &str, channel: &str, publisher: &str) -> Self {
        InstalledSnap {
            name: name.to_owned(),
            active_revision: 0,
            channel: channel.to_owned(),
            publisher: publisher.to_owned(),
            revisions: Vec::new(),
            connections: Vec::new(),
        }
    }

    /// Retrieve the active revision record.
    pub fn active_record(&self) -> Option<&InstalledRevision> {
        self.revisions.iter().find(|r| r.revision == self.active_revision)
    }

    /// Mount a newly installed or refreshed revision.
    pub fn install_revision(
        &mut self,
        container: &SnapContainer,
        manifest: SnapManifest,
    ) -> Result<(), &'static str> {
        let rev = container.revision;
        let mount_dir = format!("/snap/{}/{}", self.name, rev);
        let mut installed_files = Vec::new();

        {
            let mut vfs = VFS.lock();

            // 1. Ensure /snap and /snap/bin exist
            let _ = vfs.make_dir("/snap");
            let _ = vfs.make_dir("/snap/bin");
            let _ = vfs.make_dir(&format!("/snap/{}", self.name));
            let _ = vfs.make_dir(&mount_dir);

            // 2. Unpack read-only container files into /snap/<name>/<rev>/
            for f in &container.files {
                let full_path = format!("{}/{}", mount_dir, f.path.trim_start_matches('/'));
                if f.is_dir {
                    let _ = vfs.make_dir(&full_path);
                } else {
                    if vfs.write_file(&full_path, &f.data).is_ok() {
                        installed_files.push(full_path);
                    }
                }
            }

            // 3. Write /snap/<name>/current indicator file
            let current_symlink_path = format!("/snap/{}/current", self.name);
            let _ = vfs.write_file(&current_symlink_path, mount_dir.as_bytes());

            // 4. Create runner wrappers in /snap/bin/<app>
            for app in &manifest.apps {
                let wrapper_path = format!("/snap/bin/{}", app.name);
                let wrapper_script = format!(
                    "#!/bin/sh\n# MicroOS Snap Execution Wrapper for '{}'\nSNAP={}\nSNAP_NAME={}\nSNAP_VERSION={}\nSNAP_REVISION={}\nexec {}/{}\n",
                    app.name, mount_dir, self.name, manifest.version, rev, mount_dir, app.command
                );
                let _ = vfs.write_file(&wrapper_path, wrapper_script.as_bytes());
                installed_files.push(wrapper_path);
            }
        }

        // 5. Connect default plugs (e.g. network, home)
        for plug in &manifest.plugs {
            let plug_name = format!("{}:{}", self.name, plug);
            let slot_name = format!(":{}", plug);
            if !self.connections.iter().any(|c| c.plug == plug_name) {
                self.connections.push(InterfaceConnection {
                    plug: plug_name,
                    slot: slot_name,
                    connected: true,
                });
            }
        }

        // 6. Record revision
        self.revisions.retain(|r| r.revision != rev);
        self.revisions.push(InstalledRevision {
            revision: rev,
            version: manifest.version.clone(),
            mount_path: mount_dir,
            manifest,
            files: installed_files,
        });

        self.active_revision = rev;
        serial_println!(
            "snap: Mounted '{}' revision {} at '/snap/{}/{}'",
            self.name,
            rev,
            self.name,
            rev
        );
        Ok(())
    }

    /// Roll back to the previous available revision.
    pub fn revert(&mut self) -> Result<u32, &'static str> {
        if self.revisions.len() < 2 {
            return Err("No prior revision available to revert to");
        }

        // Sort revisions descending
        self.revisions.sort_by(|a, b| b.revision.cmp(&a.revision));

        // Find revision preceding active_revision
        let prev = self
            .revisions
            .iter()
            .find(|r| r.revision < self.active_revision)
            .ok_or("No older revision found")?;

        let target_rev = prev.revision;
        let mount_dir = prev.mount_path.clone();

        {
            let mut vfs = VFS.lock();
            let current_symlink_path = format!("/snap/{}/current", self.name);
            let _ = vfs.write_file(&current_symlink_path, mount_dir.as_bytes());

            // Re-point /snap/bin/<app> to reverted revision
            for app in &prev.manifest.apps {
                let wrapper_path = format!("/snap/bin/{}", app.name);
                let wrapper_script = format!(
                    "#!/bin/sh\n# MicroOS Snap Execution Wrapper for '{}'\nSNAP={}\nSNAP_NAME={}\nSNAP_VERSION={}\nSNAP_REVISION={}\nexec {}/{}\n",
                    app.name, mount_dir, self.name, prev.version, target_rev, mount_dir, app.command
                );
                let _ = vfs.write_file(&wrapper_path, wrapper_script.as_bytes());
            }
        }

        self.active_revision = target_rev;
        serial_println!(
            "snap: Reverted '{}' to revision {} ({})",
            self.name,
            target_rev,
            mount_dir
        );
        Ok(target_rev)
    }

    /// Purge this snap and all revisions from the filesystem.
    pub fn purge(&mut self) {
        let mut vfs = VFS.lock();

        for rev in &self.revisions {
            for file in &rev.files {
                let _ = vfs.remove(file);
            }
            let _ = vfs.remove(&rev.mount_path);
        }

        // Remove active pointer and wrappers
        let _ = vfs.remove(&format!("/snap/{}/current", self.name));
        let _ = vfs.remove(&format!("/snap/{}", self.name));

        self.revisions.clear();
        self.connections.clear();
        self.active_revision = 0;
        serial_println!("snap: Purged '{}' from system", self.name);
    }
}
