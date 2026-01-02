#![no_main]
#![no_std]

extern crate alloc;

mod config;
mod loader;
mod menu;

use config::BootConfig;
use core::time::Duration;
use uefi::fs::{FileSystem, Path};
use uefi::prelude::*;
use uefi::{boot, CString16};

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    log::info!("rust-boot starting...");

    // Get filesystem handle for ESP
    let fs = match open_esp_filesystem() {
        Ok(fs) => fs,
        Err(e) => {
            log::error!("Failed to open ESP filesystem: {:?}", e);
            return Status::LOAD_ERROR;
        }
    };

    // Load and parse config
    let config = match load_config(fs) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            return Status::LOAD_ERROR;
        }
    };

    log::info!("Loaded {} boot entries", config.entries.len());

    // Display menu and get selection
    let selected = menu::show(&config);
    let entry = &config.entries[selected];

    log::info!("Booting: {}", entry.title);
    log::info!("  kernel: {}", entry.kernel);
    for initrd in &entry.initrd {
        log::info!("  initrd: {}", initrd);
    }
    log::info!("  options: {}", entry.options);

    // Re-open filesystem for loading kernel
    let mut fs = match open_esp_filesystem() {
        Ok(fs) => fs,
        Err(e) => {
            log::error!("Failed to reopen ESP filesystem: {:?}", e);
            return Status::LOAD_ERROR;
        }
    };

    // Load and boot the kernel
    if let Err(e) = loader::boot_linux(&mut fs, entry) {
        log::error!("Boot failed: {}", e);
        log::info!("Press any key to continue...");
        boot::stall(Duration::from_secs(10));
        return Status::LOAD_ERROR;
    }

    // Should not reach here
    Status::SUCCESS
}

fn open_esp_filesystem() -> uefi::Result<FileSystem> {
    let handle = boot::image_handle();
    let fs_proto = boot::get_image_file_system(handle)?;
    Ok(FileSystem::new(fs_proto))
}

fn load_config(mut fs: FileSystem) -> Result<BootConfig, &'static str> {
    let path = CString16::try_from("\\boot.conf").map_err(|_| "Invalid path")?;
    let content = fs
        .read_to_string(Path::new(&path))
        .map_err(|_| "Failed to read boot.conf")?;

    config::parse(&content)
}
