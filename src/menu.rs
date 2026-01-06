//! Boot menu display and selection

use crate::config::{default_entry_index, BootConfig};
use core::time::Duration;
use uefi::boot::{self, TimerTrigger};
use uefi::proto::console::text::{Key, ScanCode};

/// Display the boot menu and return the index of the selected entry
pub fn show(config: &BootConfig) -> usize {
    let mut selected = default_entry_index(config);

    // If timeout is 0, boot immediately
    if config.timeout == 0 {
        return selected;
    }

    let mut remaining = config.timeout;
    display_menu(config, selected, remaining);

    // Create a timer event for 1-second intervals
    // SAFETY: We're creating a timer event without a callback (None, None)
    let timer = match unsafe {
        boot::create_event(
            boot::EventType::TIMER,
            boot::Tpl::APPLICATION,
            None,
            None,
        )
    } {
        Ok(t) => t,
        Err(e) => {
            log::error!("Failed to create timer event: {:?}", e);
            // Fallback: just wait and boot
            boot::stall(Duration::from_secs(config.timeout as u64));
            return selected;
        }
    };

    // Set timer to fire every second (periodic)
    if let Err(e) = boot::set_timer(&timer, TimerTrigger::Periodic(10_000_000)) {
        log::error!("Failed to set timer: {:?}", e);
        boot::stall(Duration::from_secs(config.timeout as u64));
        return selected;
    }

    loop {
        // Wait briefly then check for events
        boot::stall(Duration::from_millis(50));

        // Check if timer fired (poll it)
        // SAFETY: We're cloning the event to check its state, timer remains valid
        if boot::check_event(unsafe { timer.unsafe_clone() }).is_ok() {
            if remaining > 0 {
                remaining -= 1;
                display_menu(config, selected, remaining);
            }
            if remaining == 0 {
                return selected;
            }
        }

        // Check for key press
        if let Some(key) = read_key() {
            let mut handled = false;
            match key {
                Key::Special(ScanCode::UP) => {
                    if selected > 0 {
                        selected -= 1;
                        handled = true;
                    }
                }
                Key::Special(ScanCode::DOWN) => {
                    if selected < config.entries.len() - 1 {
                        selected += 1;
                        handled = true;
                    }
                }
                Key::Printable(c) if c == uefi::Char16::try_from('\r').unwrap() => {
                    return selected;
                }
                Key::Printable(c) if c == uefi::Char16::try_from(' ').unwrap() => {
                    return selected;
                }
                _ => {}
            }
            // Only reset timeout on actual navigation
            if handled {
                remaining = config.timeout;
                display_menu(config, selected, remaining);
            }
        }
    }
}

fn display_menu(config: &BootConfig, selected: usize, remaining: u32) {
    // Clear screen using UEFI console
    clear_screen();

    log::info!("===== rust-boot =====\n");
    log::info!("Use UP/DOWN to select, ENTER to boot\n");

    for (i, entry) in config.entries.iter().enumerate() {
        let marker = if i == selected { ">" } else { " " };
        log::info!("{} {}", marker, entry.title);
    }

    log::info!("");
    log::info!("Boot in {} seconds...", remaining);
}

fn clear_screen() {
    if let Ok(handle) =
        boot::get_handle_for_protocol::<uefi::proto::console::text::Output>()
    {
        if let Ok(mut stdout) =
            boot::open_protocol_exclusive::<uefi::proto::console::text::Output>(handle)
        {
            let _ = stdout.clear();
        }
    }
}

fn read_key() -> Option<Key> {
    // Try to read a key without blocking
    let stdin_handle = boot::get_handle_for_protocol::<uefi::proto::console::text::Input>().ok()?;

    let mut stdin = boot::open_protocol_exclusive::<uefi::proto::console::text::Input>(stdin_handle)
        .ok()?;

    // read_key returns Option<Key> - None if no key available
    stdin.read_key().ok().flatten()
}
