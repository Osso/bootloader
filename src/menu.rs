//! Boot menu display and selection

use crate::config::{BootConfig, default_entry_index};
use uefi::proto::console::text::Key;

/// Display the boot menu and return the index of the selected entry
pub fn show(config: &BootConfig) -> usize {
    let default = default_entry_index(config);
    display_menu(config, default);

    loop {
        let Some(selected) = selected_entry(read_key(), config.entries.len(), default) else {
            continue;
        };
        return selected;
    }
}

fn selected_entry(key: Option<Key>, entry_count: usize, default: usize) -> Option<usize> {
    let Key::Printable(printable) = key? else {
        return None;
    };
    let ch = char::from(printable);
    if ch == '\r' {
        return Some(default);
    }

    let digit = ch.to_digit(10)?;
    let index = (digit as usize).saturating_sub(1);
    if index < entry_count {
        return Some(index);
    }
    None
}

fn display_menu(config: &BootConfig, default: usize) {
    clear_screen();

    log::info!("=== bootloader ===\n");

    for (i, entry) in config.entries.iter().enumerate() {
        let marker = if i == default { "*" } else { " " };
        log::info!(" {}{} {}", i + 1, marker, entry.title);
    }

    log::info!("");
    log::info!(
        "Press 1-{} to boot, Enter for default",
        config.entries.len()
    );
}

fn clear_screen() {
    uefi::system::with_stdout(|stdout| {
        let _ = stdout.clear();
    });
}

fn read_key() -> Option<Key> {
    use uefi::boot;

    let stdin_handle = boot::get_handle_for_protocol::<uefi::proto::console::text::Input>().ok()?;

    let mut stdin =
        boot::open_protocol_exclusive::<uefi::proto::console::text::Input>(stdin_handle).ok()?;

    stdin.read_key().ok().flatten()
}
