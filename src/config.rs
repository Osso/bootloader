//! Config file parser for boot.conf
//!
//! Format:
//! ```ini
//! timeout = 5
//! default = arch
//!
//! [arch]
//! title = Arch Linux
//! kernel = /arch/vmlinuz-linux
//! initrd = /arch/amd-ucode.img
//! initrd = /arch/initramfs-linux.img
//! options = root=UUID=... rw quiet
//! ```

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct BootConfig {
    pub timeout: u32,
    pub default: String,
    pub entries: Vec<BootEntry>,
}

#[derive(Debug)]
pub struct BootEntry {
    pub id: String,
    pub title: String,
    pub kernel: String,
    pub initrd: Vec<String>,
    pub options: String,
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            timeout: 5,
            default: String::new(),
            entries: Vec::new(),
        }
    }
}

impl Default for BootEntry {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            kernel: String::new(),
            initrd: Vec::new(),
            options: String::new(),
        }
    }
}

pub fn parse(content: &str) -> Result<BootConfig, &'static str> {
    let mut config = BootConfig::default();
    let mut current_entry: Option<BootEntry> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Section header [name]
        if line.starts_with('[') && line.ends_with(']') {
            // Save previous entry if any
            if let Some(entry) = current_entry.take() {
                if !entry.kernel.is_empty() {
                    config.entries.push(entry);
                }
            }

            let id = &line[1..line.len() - 1];
            let mut entry = BootEntry::default();
            entry.id = String::from(id);
            current_entry = Some(entry);
            continue;
        }

        // Key = value pairs
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            if let Some(ref mut entry) = current_entry {
                // Inside a section
                match key {
                    "title" => entry.title = String::from(value),
                    "kernel" => entry.kernel = String::from(value),
                    "initrd" => entry.initrd.push(String::from(value)),
                    "options" => entry.options = String::from(value),
                    _ => {} // Ignore unknown keys
                }
            } else {
                // Global settings
                match key {
                    "timeout" => {
                        config.timeout = value.parse().unwrap_or(5);
                    }
                    "default" => {
                        config.default = String::from(value);
                    }
                    _ => {} // Ignore unknown keys
                }
            }
        }
    }

    // Save last entry
    if let Some(entry) = current_entry {
        if !entry.kernel.is_empty() {
            config.entries.push(entry);
        }
    }

    if config.entries.is_empty() {
        return Err("No boot entries found");
    }

    // Set default if not specified
    if config.default.is_empty() {
        config.default = config.entries[0].id.clone();
    }

    Ok(config)
}

/// Find the index of the default entry
pub fn default_entry_index(config: &BootConfig) -> usize {
    config
        .entries
        .iter()
        .position(|e| e.id == config.default)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = r#"
timeout = 3
default = arch

[arch]
title = Arch Linux
kernel = /arch/vmlinuz-linux
initrd = /arch/amd-ucode.img
initrd = /arch/initramfs-linux.img
options = root=UUID=abc123 rw quiet
"#;

        let config = parse(content).unwrap();
        assert_eq!(config.timeout, 3);
        assert_eq!(config.default, "arch");
        assert_eq!(config.entries.len(), 1);

        let entry = &config.entries[0];
        assert_eq!(entry.id, "arch");
        assert_eq!(entry.title, "Arch Linux");
        assert_eq!(entry.kernel, "/arch/vmlinuz-linux");
        assert_eq!(entry.initrd.len(), 2);
        assert_eq!(entry.options, "root=UUID=abc123 rw quiet");
    }
}
