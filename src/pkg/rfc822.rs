//! MicroOS Debian RFC 822 / Control File Parser
//!
//! Parses Debian `control` files and multi-paragraph `Packages` index files.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

/// Parsed package metadata from an RFC 822 control record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    pub architecture: String,
    pub maintainer: String,
    pub depends: Vec<String>,
    pub filename: String,
    pub size: usize,
    pub sha256: String,
    pub description: String,
    pub section: String,
    pub priority: String,
}

impl PackageMeta {
    pub fn new(name: &str, version: &str) -> Self {
        PackageMeta {
            name: name.to_owned(),
            version: version.to_owned(),
            architecture: "x86_64".to_owned(),
            maintainer: "MicroOS Team <team@microos.org>".to_owned(),
            depends: Vec::new(),
            filename: alloc::format!("pool/main/{}/{}_{}_x86_64.deb", name, name, version),
            size: 1024,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
            description: alloc::format!("Package {} for MicroOS", name),
            section: "utils".to_owned(),
            priority: "optional".to_owned(),
        }
    }
}

/// Parse a multi-paragraph Debian `Packages` repository index.
pub fn parse_packages_index(input: &str) -> Vec<PackageMeta> {
    let mut packages = Vec::new();
    let paragraphs = input.split("\n\n");

    for p in paragraphs {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(meta) = parse_control_paragraph(trimmed) {
            packages.push(meta);
        }
    }

    packages
}

/// Parse a single RFC 822 control paragraph into `PackageMeta`.
pub fn parse_control_paragraph(paragraph: &str) -> Option<PackageMeta> {
    let mut name = String::new();
    let mut version = String::new();
    let mut architecture = "x86_64".to_owned();
    let mut maintainer = String::new();
    let mut depends = Vec::new();
    let mut filename = String::new();
    let mut size = 0;
    let mut sha256 = String::new();
    let mut description = String::new();
    let mut section = "utils".to_owned();
    let mut priority = "optional".to_owned();

    let mut current_field: Option<&str> = None;

    for line in paragraph.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line for multi-line fields (e.g. Description)
            if let Some(field) = current_field {
                if field == "Description" {
                    let cont = line.trim();
                    if cont == "." {
                        description.push('\n');
                    } else {
                        if !description.is_empty() && !description.ends_with('\n') {
                            description.push(' ');
                        }
                        description.push_str(cont);
                    }
                }
            }
            continue;
        }

        if let Some(idx) = line.find(':') {
            let key = line[..idx].trim();
            let val = line[idx + 1..].trim();
            current_field = Some(key);

            match key {
                "Package" => name = val.to_owned(),
                "Version" => version = val.to_owned(),
                "Architecture" => architecture = val.to_owned(),
                "Maintainer" => maintainer = val.to_owned(),
                "Depends" => {
                    for dep in val.split(',') {
                        let dep_name = dep.trim();
                        // Strip version constraints if present, e.g. "libc-micro (>= 1.0)"
                        let clean_dep = if let Some(space_idx) = dep_name.find(' ') {
                            &dep_name[..space_idx]
                        } else {
                            dep_name
                        };
                        if !clean_dep.is_empty() {
                            depends.push(clean_dep.to_owned());
                        }
                    }
                }
                "Filename" => filename = val.to_owned(),
                "Size" => size = val.parse().unwrap_or(0),
                "SHA256" => sha256 = val.to_owned(),
                "Description" => description = val.to_owned(),
                "Section" => section = val.to_owned(),
                "Priority" => priority = val.to_owned(),
                _ => {}
            }
        }
    }

    if name.is_empty() {
        None
    } else {
        Some(PackageMeta {
            name,
            version: if version.is_empty() { "1.0.0".to_owned() } else { version },
            architecture,
            maintainer,
            depends,
            filename,
            size,
            sha256,
            description,
            section,
            priority,
        })
    }
}
