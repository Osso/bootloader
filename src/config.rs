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
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(section_id) = section_id(line) {
            push_entry(&mut config, current_entry.take());
            current_entry = Some(new_entry(section_id));
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            apply_key_value(&mut config, &mut current_entry, key.trim(), value.trim());
        }
    }

    push_entry(&mut config, current_entry);
    if config.entries.is_empty() {
        return Err("No boot entries found");
    }
    if config.default.is_empty() {
        config.default = config.entries[0].id.clone();
    }
    Ok(config)
}

fn section_id(line: &str) -> Option<&str> {
    line.strip_prefix('[')?.strip_suffix(']')
}

fn new_entry(id: &str) -> BootEntry {
    BootEntry {
        id: String::from(id),
        ..BootEntry::default()
    }
}

fn push_entry(config: &mut BootConfig, entry: Option<BootEntry>) {
    let Some(entry) = entry else {
        return;
    };
    if !entry.kernel.is_empty() {
        config.entries.push(entry);
    }
}

fn apply_key_value(
    config: &mut BootConfig,
    current_entry: &mut Option<BootEntry>,
    key: &str,
    value: &str,
) {
    match current_entry {
        Some(entry) => apply_entry_key(entry, key, value),
        None => apply_global_key(config, key, value),
    }
}

fn apply_entry_key(entry: &mut BootEntry, key: &str, value: &str) {
    match key {
        "title" => entry.title = String::from(value),
        "kernel" => entry.kernel = String::from(value),
        "initrd" => entry.initrd.push(String::from(value)),
        "options" => entry.options = String::from(value),
        _ => {}
    }
}

fn apply_global_key(config: &mut BootConfig, key: &str, value: &str) {
    if key == "default" {
        config.default = String::from(value);
    }
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
