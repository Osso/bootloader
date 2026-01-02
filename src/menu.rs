//! Boot menu display and selection

use crate::config::{default_entry_index, BootConfig};
use core::time::Duration;
use uefi::boot;
use uefi::proto::console::text::{Key, ScanCode};

/// Display the boot menu and return the index of the selected entry
pub fn show(config: &BootConfig) -> usize {
    let mut selected = default_entry_index(config);

    log::info!("\n");
    log::info!("===== rust-boot =====\n");

    // If timeout is 0, boot immediately
    if config.timeout == 0 {
        return selected;
    }

    // Display entries
    display_menu(config, selected);

    // Wait for input or timeout
    let mut remaining = config.timeout;

    loop {
        // Check for key press
        if let Some(key) = read_key() {
            match key {
                Key::Special(ScanCode::UP) => {
                    if selected > 0 {
                        selected -= 1;
                        display_menu(config, selected);
                    }
                }
                Key::Special(ScanCode::DOWN) => {
                    if selected < config.entries.len() - 1 {
                        selected += 1;
                        display_menu(config, selected);
                    }
                }
                Key::Printable(c) if c == uefi::Char16::try_from('\r').unwrap() => {
                    // Enter pressed
                    return selected;
                }
                Key::Printable(c) if c == uefi::Char16::try_from(' ').unwrap() => {
                    // Space also selects
                    return selected;
                }
                _ => {}
            }
            // Reset timeout on any keypress
            remaining = config.timeout;
        }

        // Simple timeout check - stall for 100ms intervals
        boot::stall(Duration::from_millis(100));

        // Decrement remaining time (roughly)
        if remaining > 0 {
            // This is approximate - we check every ~100ms
            static mut COUNTER: u32 = 0;
            unsafe {
                COUNTER += 1;
                if COUNTER >= 10 {
                    COUNTER = 0;
                    remaining -= 1;
                    log::info!("Boot in {} seconds...", remaining);
                }
            }
        }

        if remaining == 0 {
            return selected;
        }
    }
}

fn display_menu(config: &BootConfig, selected: usize) {
    log::info!("\x1b[2J\x1b[H"); // Clear screen (ANSI)
    log::info!("===== rust-boot =====\n");
    log::info!("Use UP/DOWN to select, ENTER to boot\n");

    for (i, entry) in config.entries.iter().enumerate() {
        let marker = if i == selected { ">" } else { " " };
        log::info!("{} {}", marker, entry.title);
    }
    log::info!("");
}

fn read_key() -> Option<Key> {
    // Try to read a key without blocking
    let stdin_handle = boot::get_handle_for_protocol::<uefi::proto::console::text::Input>().ok()?;

    let mut stdin = boot::open_protocol_exclusive::<uefi::proto::console::text::Input>(stdin_handle)
        .ok()?;

    // read_key returns Option<Key> - None if no key available
    stdin.read_key().ok().flatten()
}
