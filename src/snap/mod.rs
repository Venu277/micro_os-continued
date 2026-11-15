//! MicroOS Native Snap Store & Package Container Subsystem
//!
//! Provides containerized snap package management, including remote store
//! catalog searching, `.snap` container generation/installation, revision
//! tracking, instant rollback (`snap revert`), and execution sandbox simulation.

pub mod container;
pub mod environment;
pub mod manifest;

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;
use crate::snap::container::{build_snap, parse_snap};
use crate::snap::environment::InstalledSnap;
use crate::snap::manifest::{Confinement, SnapApp, SnapManifest};

pub static SNAP_MANAGER: Mutex<Option<SnapManager>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub struct StorePackage {
    pub name: String,
    pub version: String,
    pub revision: u32,
    pub summary: String,
    pub description: String,
    pub publisher: String,
    pub channel: String,
    pub confinement: Confinement,
    pub plugs: Vec<String>,
    pub apps: Vec<SnapApp>,
    pub files: Vec<(String, bool, u16, Vec<u8>)>,
}

pub struct SnapManager {
    pub store_catalog: Vec<StorePackage>,
    pub installed: Vec<InstalledSnap>,
}

impl SnapManager {
    pub fn new() -> Self {
        SnapManager {
            store_catalog: Vec::new(),
            installed: Vec::new(),
        }
    }

    /// Search store catalog by name or summary substring.
    pub fn find(&self, query: &str) -> Vec<&StorePackage> {
        let q = query.trim().to_lowercase();
        self.store_catalog
            .iter()
            .filter(|p| {
                q.is_empty()
                    || p.name.to_lowercase().contains(&q)
                    || p.summary.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Get store package metadata by exact name.
    pub fn info(&self, name: &str) -> Option<&StorePackage> {
        self.store_catalog.iter().find(|p| p.name == name)
    }

    /// Install a snap package from the store catalog.
    pub fn install(
        &mut self,
        name: &str,
        channel: Option<&str>,
        classic: bool,
    ) -> Result<(), &'static str> {
        let store_pkg = self.info(name).ok_or("Snap package not found in store")?.clone();

        if store_pkg.confinement == Confinement::Classic && !classic {
            return Err("This snap requires classic confinement: pass '--classic'");
        }

        // Build .snap container image from package files
        let manifest = SnapManifest {
            name: store_pkg.name.clone(),
            version: store_pkg.version.clone(),
            summary: store_pkg.summary.clone(),
            description: store_pkg.description.clone(),
            base: Some("core22".to_owned()),
            confinement: store_pkg.confinement,
            grade: "stable".to_owned(),
            apps: store_pkg.apps.clone(),
            plugs: store_pkg.plugs.clone(),
            slots: Vec::new(),
        };

        let yaml = manifest.to_yaml();
        let mut file_tuples: Vec<(&str, bool, u16, &[u8])> = Vec::new();
        file_tuples.push(("meta/snap.yaml", false, 0o644, yaml.as_bytes()));

        for f in &store_pkg.files {
            file_tuples.push((&f.0, f.1, f.2, &f.3));
        }

        let snap_bytes = build_snap(
            &store_pkg.name,
            &store_pkg.version,
            store_pkg.revision,
            &file_tuples,
        );

        let container = parse_snap(&snap_bytes)?;

        // Find or create installed snap record
        let target_channel = channel.unwrap_or(&store_pkg.channel);
        let pos = self.installed.iter().position(|s| s.name == store_pkg.name);

        let installed_snap = match pos {
            Some(idx) => &mut self.installed[idx],
            None => {
                self.installed.push(InstalledSnap::new(
                    &store_pkg.name,
                    target_channel,
                    &store_pkg.publisher,
                ));
                let last_idx = self.installed.len() - 1;
                &mut self.installed[last_idx]
            }
        };

        installed_snap.install_revision(&container, manifest)
    }

    /// Refresh (update) an installed snap to a newer revision if available.
    pub fn refresh(&mut self, name: Option<&str>) -> Result<Vec<(String, u32)>, &'static str> {
        let mut refreshed = Vec::new();
        let names_to_check: Vec<String> = match name {
            Some(n) => vec![n.to_owned()],
            None => self.installed.iter().map(|s| s.name.clone()).collect(),
        };

        for snap_name in names_to_check {
            let store_pkg = match self.info(&snap_name) {
                Some(p) => p.clone(),
                None => continue,
            };

            let installed = match self.installed.iter().find(|s| s.name == snap_name) {
                Some(i) => i,
                None => continue,
            };

            // Check if store has a higher revision
            if store_pkg.revision > installed.active_revision {
                let higher_rev = store_pkg.revision;
                self.install(&snap_name, None, store_pkg.confinement == Confinement::Classic)?;
                refreshed.push((snap_name, higher_rev));
            }
        }

        Ok(refreshed)
    }

    /// Revert a snap to its previous installed revision.
    pub fn revert(&mut self, name: &str) -> Result<u32, &'static str> {
        let installed = self
            .installed
            .iter_mut()
            .find(|s| s.name == name)
            .ok_or("Snap package is not installed")?;

        installed.revert()
    }

    /// Remove an installed snap completely.
    pub fn remove(&mut self, name: &str) -> Result<(), &'static str> {
        let pos = self
            .installed
            .iter()
            .position(|s| s.name == name)
            .ok_or("Snap package is not installed")?;

        let mut snap = self.installed.remove(pos);
        snap.purge();
        Ok(())
    }

    /// List all installed snaps.
    pub fn list(&self) -> &[InstalledSnap] {
        &self.installed
    }

    /// Execute an installed snap application within the simulated sandbox.
    pub fn run(&self, app_name: &str, args: &[&str]) -> Result<String, &'static str> {
        // Find installed snap providing this app
        let snap = self
            .installed
            .iter()
            .find(|s| {
                if let Some(rec) = s.active_record() {
                    rec.manifest.apps.iter().any(|a| a.name == app_name)
                } else {
                    false
                }
            })
            .ok_or("Snap application not found or not installed")?;

        let rec = snap.active_record().ok_or("No active revision mounted")?;
        let app = rec
            .manifest
            .apps
            .iter()
            .find(|a| a.name == app_name)
            .ok_or("App definition missing")?;

        let mut output = String::new();
        output.push_str(&format!(
            "[{}] Running sandboxed snap '{}' (rev {}, confinement={})\n",
            app.name,
            snap.name,
            rec.revision,
            rec.manifest.confinement.as_str()
        ));

        // Plugs verification
        if !rec.manifest.plugs.is_empty() {
            output.push_str("Connected Interfaces:\n");
            for plug in &rec.manifest.plugs {
                output.push_str(&format!("  * {}:{} -> :{}\n", snap.name, plug, plug));
            }
        }

        output.push_str("---\n");

        // Built-in sandboxed runtime logic based on app
        match app.name.as_str() {
            "micro-calc" => {
                if args.is_empty() {
                    output.push_str("micro-calc v1.4: Usage: snap run micro-calc <num1> <op> <num2>\nExample: snap run micro-calc 15 + 27\n");
                } else if args.len() >= 3 {
                    let n1: i64 = args[0].parse().unwrap_or(0);
                    let op = args[1];
                    let n2: i64 = args[2].parse().unwrap_or(0);
                    let res = match op {
                        "+" => n1.wrapping_add(n2),
                        "-" => n1.wrapping_sub(n2),
                        "*" | "x" => n1.wrapping_mul(n2),
                        "/" => if n2 != 0 { n1 / n2 } else { 0 },
                        "%" => if n2 != 0 { n1 % n2 } else { 0 },
                        _ => 0,
                    };
                    output.push_str(&format!("{} {} {} = {}\n", n1, op, n2, res));
                } else {
                    output.push_str(&format!("micro-calc: evaluated expression: {}\n", args.join(" ")));
                }
            }
            "system-monitor" => {
                output.push_str("System Monitor (Sandboxed via :hardware-observe)\n");
                output.push_str("  CPU: Intel(R) Core(TM) i5-1335U (10 cores, x86_64)\n");
                output.push_str("  Kernel Heap: 256 KiB mapped at 0x444444440000\n");
                output.push_str("  Storage: NVMe 512GB (Profile) + ATA 4MiB (IDE)\n");
                output.push_str("  Network: eth0 (10.0.2.15/24) + wlan0 (MicroOS-Lab-5G)\n");
            }
            "nano-editor" => {
                let file = if args.is_empty() { "untitled.txt" } else { args[0] };
                output.push_str(&format!("nano-editor v2.1: Initialized buffer for '{}' (plugs: home, removable-media)\n[Buffer Ready. Press Ctrl+X to exit]\n", file));
            }
            "matrix-client" => {
                output.push_str("Matrix Terminal Client v0.8.4\n");
                output.push_str("  Homeserver: https://matrix.microos.org\n");
                output.push_str("  Status: Connected (:network interface granted)\n");
                output.push_str("  Rooms: #general:microos.org (14 members online)\n");
            }
            "core22" => {
                output.push_str("core22: Canonical Ubuntu 22.04 LTS minimal base runtime layer for MicroOS snaps.\n");
            }
            _ => {
                output.push_str(&format!("Executed '{}' with arguments: {:?}\n", app.command, args));
            }
        }

        Ok(output)
    }
}

/// Initialize the Snap Store subsystem with preloaded standard store snaps.
pub fn init_snap() {
    let mut mgr = SnapManager::new();

    // 1. core22 (Base Snap)
    mgr.store_catalog.push(StorePackage {
        name: "core22".to_owned(),
        version: "2026.01".to_owned(),
        revision: 1,
        summary: "Canonical base runtime environment for MicroOS snaps".to_owned(),
        description: "Provides standard C runtime, core libraries, and ABI definitions for snap applications.".to_owned(),
        publisher: "canonical*".to_owned(),
        channel: "stable".to_owned(),
        confinement: Confinement::Strict,
        plugs: Vec::new(),
        apps: vec![SnapApp {
            name: "core22".to_owned(),
            command: "bin/core22".to_owned(),
            daemon: None,
            plugs: Vec::new(),
        }],
        files: vec![
            ("bin/core22".to_owned(), false, 0o755, b"CORE22_BASE_IMAGE_PAYLOAD".to_vec()),
            ("lib/libc.so".to_owned(), false, 0o644, b"LIBC_ELF_RUNTIME_STUB".to_vec()),
        ],
    });

    // 2. micro-calc (Expression Calculator)
    mgr.store_catalog.push(StorePackage {
        name: "micro-calc".to_owned(),
        version: "1.4.0".to_owned(),
        revision: 3,
        summary: "Lightweight RPN and arithmetic calculator for terminal".to_owned(),
        description: "Fast arbitrary-precision command line calculator running in a secure sandbox.".to_owned(),
        publisher: "micro-devs".to_owned(),
        channel: "stable".to_owned(),
        confinement: Confinement::Strict,
        plugs: vec!["network".to_owned()],
        apps: vec![SnapApp {
            name: "micro-calc".to_owned(),
            command: "bin/calc".to_owned(),
            daemon: None,
            plugs: vec!["network".to_owned()],
        }],
        files: vec![
            ("bin/calc".to_owned(), false, 0o755, b"CALC_BINARY_EXECUTABLE_BYTES".to_vec()),
        ],
    });

    // 3. nano-editor (Sandboxed text editor)
    mgr.store_catalog.push(StorePackage {
        name: "nano-editor".to_owned(),
        version: "2.1.0".to_owned(),
        revision: 5,
        summary: "Compact modal text editor with syntax highlighting".to_owned(),
        description: "User-friendly editor confined to home directory and external storage.".to_owned(),
        publisher: "micro-tools".to_owned(),
        channel: "stable".to_owned(),
        confinement: Confinement::Strict,
        plugs: vec!["home".to_owned(), "removable-media".to_owned()],
        apps: vec![SnapApp {
            name: "nano-editor".to_owned(),
            command: "bin/nano".to_owned(),
            daemon: None,
            plugs: vec!["home".to_owned(), "removable-media".to_owned()],
        }],
        files: vec![
            ("bin/nano".to_owned(), false, 0o755, b"NANO_EDITOR_EXECUTABLE_BYTES".to_vec()),
        ],
    });

    // 4. system-monitor (Resource health monitor)
    mgr.store_catalog.push(StorePackage {
        name: "system-monitor".to_owned(),
        version: "1.0.2".to_owned(),
        revision: 2,
        summary: "Real-time CPU, ACPI, NVMe, and memory monitor".to_owned(),
        description: "Terminal utility inspecting laptop hardware profiles and kernel metrics.".to_owned(),
        publisher: "micro-ops".to_owned(),
        channel: "stable".to_owned(),
        confinement: Confinement::Strict,
        plugs: vec!["hardware-observe".to_owned(), "network".to_owned()],
        apps: vec![SnapApp {
            name: "system-monitor".to_owned(),
            command: "bin/sysmon".to_owned(),
            daemon: None,
            plugs: vec!["hardware-observe".to_owned()],
        }],
        files: vec![
            ("bin/sysmon".to_owned(), false, 0o755, b"SYSMON_EXECUTABLE_BYTES".to_vec()),
        ],
    });

    // 5. matrix-client (Terminal Matrix chat)
    mgr.store_catalog.push(StorePackage {
        name: "matrix-client".to_owned(),
        version: "0.8.4".to_owned(),
        revision: 4,
        summary: "Lightweight Matrix terminal chat client".to_owned(),
        description: "Secure, federated instant messaging client running sandboxed over smoltcp.".to_owned(),
        publisher: "matrix-org".to_owned(),
        channel: "candidate".to_owned(),
        confinement: Confinement::Strict,
        plugs: vec!["network".to_owned(), "home".to_owned()],
        apps: vec![SnapApp {
            name: "matrix-client".to_owned(),
            command: "bin/matrix".to_owned(),
            daemon: None,
            plugs: vec!["network".to_owned(), "home".to_owned()],
        }],
        files: vec![
            ("bin/matrix".to_owned(), false, 0o755, b"MATRIX_CLIENT_EXECUTABLE_BYTES".to_vec()),
        ],
    });

    let count = mgr.store_catalog.len();
    *SNAP_MANAGER.lock() = Some(mgr);
    serial_println!("snap: Snap Store initialized ({} packages available)", count);
}
