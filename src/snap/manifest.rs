//! MicroOS Snap Manifest (`meta/snap.yaml`) Parser
//!
//! Models the package specification for sandboxed Snap applications,
//! including security confinement modes and plug/slot interface declarations.

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confinement {
    Strict,
    Classic,
    Devmode,
}

impl Confinement {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confinement::Strict => "strict",
            Confinement::Classic => "classic",
            Confinement::Devmode => "devmode",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "classic" => Confinement::Classic,
            "devmode" => Confinement::Devmode,
            _ => Confinement::Strict,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapApp {
    pub name: String,
    pub command: String,
    pub daemon: Option<String>,
    pub plugs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapManifest {
    pub name: String,
    pub version: String,
    pub summary: String,
    pub description: String,
    pub base: Option<String>,
    pub confinement: Confinement,
    pub grade: String,
    pub apps: Vec<SnapApp>,
    pub plugs: Vec<String>,
    pub slots: Vec<String>,
}

impl SnapManifest {
    pub fn new(name: &str, version: &str) -> Self {
        SnapManifest {
            name: name.to_owned(),
            version: version.to_owned(),
            summary: String::new(),
            description: String::new(),
            base: None,
            confinement: Confinement::Strict,
            grade: "stable".to_owned(),
            apps: Vec::new(),
            plugs: Vec::new(),
            slots: Vec::new(),
        }
    }

    /// Parse a standard `meta/snap.yaml` text document.
    pub fn parse(yaml_content: &str) -> Result<Self, &'static str> {
        let mut manifest = SnapManifest::new("", "");
        let mut current_section = "";
        let mut current_app: Option<SnapApp> = None;

        for raw_line in yaml_content.lines() {
            let line = raw_line.trim_end();
            if line.trim().is_empty() || line.trim().starts_with('#') {
                continue;
            }

            // Top-level keys (no indent)
            if !line.starts_with(' ') && !line.starts_with('\t') {
                // If we were parsing an app, commit it
                if let Some(app) = current_app.take() {
                    manifest.apps.push(app);
                }

                if let Some(colon) = line.find(':') {
                    let key = line[..colon].trim();
                    let val = line[colon + 1..].trim();

                    match key {
                        "name" => manifest.name = val.to_owned(),
                        "version" => manifest.version = val.to_owned(),
                        "summary" => manifest.summary = val.to_owned(),
                        "description" => manifest.description = val.to_owned(),
                        "base" => manifest.base = if val.is_empty() { None } else { Some(val.to_owned()) },
                        "confinement" => manifest.confinement = Confinement::from_str(val),
                        "grade" => manifest.grade = if val.is_empty() { "stable".to_owned() } else { val.to_owned() },
                        "apps" => current_section = "apps",
                        "plugs" => current_section = "plugs",
                        "slots" => current_section = "slots",
                        _ => current_section = "",
                    }
                }
            } else {
                // Indented lines
                let trimmed = line.trim();
                match current_section {
                    "apps" => {
                        // Check if line defines a new app name (e.g. "  micro-calc:")
                        if line.starts_with("  ") && !line.starts_with("    ") && trimmed.ends_with(':') {
                            if let Some(app) = current_app.take() {
                                manifest.apps.push(app);
                            }
                            let app_name = trimmed.trim_end_matches(':').trim();
                            current_app = Some(SnapApp {
                                name: app_name.to_owned(),
                                command: String::new(),
                                daemon: None,
                                plugs: Vec::new(),
                            });
                        } else if let Some(app) = current_app.as_mut() {
                            if let Some(colon) = trimmed.find(':') {
                                let k = trimmed[..colon].trim();
                                let v = trimmed[colon + 1..].trim();
                                match k {
                                    "command" => app.command = v.to_owned(),
                                    "daemon" => app.daemon = Some(v.to_owned()),
                                    _ => {}
                                }
                            } else if trimmed.starts_with("- ") {
                                let plug = trimmed[2..].trim();
                                if !plug.is_empty() {
                                    app.plugs.push(plug.to_owned());
                                }
                            }
                        }
                    }
                    "plugs" => {
                        if trimmed.starts_with("- ") {
                            manifest.plugs.push(trimmed[2..].trim().to_owned());
                        } else if trimmed.ends_with(':') {
                            manifest.plugs.push(trimmed.trim_end_matches(':').trim().to_owned());
                        }
                    }
                    "slots" => {
                        if trimmed.starts_with("- ") {
                            manifest.slots.push(trimmed[2..].trim().to_owned());
                        } else if trimmed.ends_with(':') {
                            manifest.slots.push(trimmed.trim_end_matches(':').trim().to_owned());
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(app) = current_app {
            manifest.apps.push(app);
        }

        if manifest.name.is_empty() {
            return Err("Snap manifest missing 'name'");
        }
        if manifest.version.is_empty() {
            return Err("Snap manifest missing 'version'");
        }

        Ok(manifest)
    }

    /// Serialize this manifest back to YAML.
    pub fn to_yaml(&self) -> String {
        use alloc::format;
        let mut out = String::new();
        out.push_str(&format!("name: {}\n", self.name));
        out.push_str(&format!("version: {}\n", self.version));
        out.push_str(&format!("summary: {}\n", self.summary));
        out.push_str(&format!("description: {}\n", self.description));
        if let Some(b) = &self.base {
            out.push_str(&format!("base: {}\n", b));
        }
        out.push_str(&format!("confinement: {}\n", self.confinement.as_str()));
        out.push_str(&format!("grade: {}\n", self.grade));

        if !self.apps.is_empty() {
            out.push_str("apps:\n");
            for app in &self.apps {
                out.push_str(&format!("  {}:\n", app.name));
                out.push_str(&format!("    command: {}\n", app.command));
                if let Some(d) = &app.daemon {
                    out.push_str(&format!("    daemon: {}\n", d));
                }
                if !app.plugs.is_empty() {
                    out.push_str("    plugs:\n");
                    for p in &app.plugs {
                        out.push_str(&format!("      - {}\n", p));
                    }
                }
            }
        }

        if !self.plugs.is_empty() {
            out.push_str("plugs:\n");
            for p in &self.plugs {
                out.push_str(&format!("  - {}\n", p));
            }
        }

        if !self.slots.is_empty() {
            out.push_str("slots:\n");
            for s in &self.slots {
                out.push_str(&format!("  - {}\n", s));
            }
        }

        out
    }
}
