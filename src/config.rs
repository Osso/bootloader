//! Config file parser for boot.conf
//!
//! Format:
//! ```ini
//! default = arch
//!
//! [arch]
//! title = Arch Linux
//! kernel = /arch/vmlinuz-linux
//! initrd = /arch/amd-ucode.img
//! initrd = /arch/initramfs-linux.img
//! options = root=UUID=... rw quiet
//! ```

#[cfg(target_os = "uefi")]
use alloc::string::String;
#[cfg(target_os = "uefi")]
use alloc::vec::Vec;

#[cfg(not(target_os = "uefi"))]
use std::string::String;
#[cfg(not(target_os = "uefi"))]
use std::vec::Vec;

#[derive(Debug)]
pub struct BootConfig {
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
                if key == "default" {
                    config.default = String::from(value);
                }
                // Ignore unknown keys (including legacy "timeout")
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
default = arch

[arch]
title = Arch Linux
kernel = /arch/vmlinuz-linux
initrd = /arch/amd-ucode.img
initrd = /arch/initramfs-linux.img
options = root=UUID=abc123 rw quiet
"#;

        let config = parse(content).unwrap();
        assert_eq!(config.default, "arch");
        assert_eq!(config.entries.len(), 1);

        let entry = &config.entries[0];
        assert_eq!(entry.id, "arch");
        assert_eq!(entry.title, "Arch Linux");
        assert_eq!(entry.kernel, "/arch/vmlinuz-linux");
        assert_eq!(entry.initrd.len(), 2);
        assert_eq!(entry.options, "root=UUID=abc123 rw quiet");
    }

    #[test]
    fn test_multiple_entries() {
        let content = r#"
default = ubuntu

[arch]
title = Arch Linux
kernel = /arch/vmlinuz
initrd = /arch/initramfs.img
options = root=/dev/sda1

[ubuntu]
title = Ubuntu
kernel = /ubuntu/vmlinuz
initrd = /ubuntu/initrd.img
options = root=/dev/sda2
"#;

        let config = parse(content).unwrap();
        assert_eq!(config.entries.len(), 2);
        assert_eq!(config.entries[0].id, "arch");
        assert_eq!(config.entries[1].id, "ubuntu");
    }

    #[test]
    fn test_default_entry_index() {
        let content = r#"
default = ubuntu

[arch]
title = Arch
kernel = /vmlinuz
options = root=/dev/sda1

[ubuntu]
title = Ubuntu
kernel = /vmlinuz
options = root=/dev/sda2
"#;

        let config = parse(content).unwrap();
        assert_eq!(default_entry_index(&config), 1);
    }

    #[test]
    fn test_default_entry_not_found() {
        let content = r#"
default = nonexistent

[arch]
title = Arch
kernel = /vmlinuz
options = root=/dev/sda1
"#;

        let config = parse(content).unwrap();
        assert_eq!(default_entry_index(&config), 0); // Falls back to first
    }

    #[test]
    fn test_no_initrd() {
        let content = r#"
[minimal]
title = Minimal
kernel = /vmlinuz
options = root=/dev/sda1
"#;

        let config = parse(content).unwrap();
        assert_eq!(config.entries[0].initrd.len(), 0);
    }

    #[test]
    fn test_defaults() {
        let content = r#"
[test]
title = Test
kernel = /vmlinuz
"#;

        let config = parse(content).unwrap();
        assert_eq!(config.default, "test"); // defaults to first entry
        assert_eq!(config.entries[0].options, ""); // default
    }

    #[test]
    fn test_whitespace_handling() {
        let content = "  default  =  test  \n[test]\n  title  =  Spaced Title  \nkernel=/vmlinuz\n";

        let config = parse(content).unwrap();
        assert_eq!(config.default, "test");
        assert_eq!(config.entries[0].title, "Spaced Title");
    }

    #[test]
    fn test_empty_lines_ignored() {
        let content = r#"

default = test

[test]

title = Test

kernel = /vmlinuz

"#;

        let config = parse(content).unwrap();
        assert_eq!(config.default, "test");
        assert_eq!(config.entries.len(), 1);
    }

    #[test]
    fn test_missing_kernel_fails() {
        let content = r#"
[test]
title = Test
options = root=/dev/sda1
"#;

        assert!(parse(content).is_err());
    }
}
