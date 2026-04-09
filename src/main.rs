#![cfg_attr(target_os = "uefi", no_main)]
#![cfg_attr(target_os = "uefi", no_std)]

#[cfg(target_os = "uefi")]
extern crate alloc;

mod config;
#[cfg(target_os = "uefi")]
mod loader;
#[cfg(target_os = "uefi")]
mod menu;

#[cfg(target_os = "uefi")]
use config::BootConfig;
#[cfg(target_os = "uefi")]
use core::time::Duration;
#[cfg(target_os = "uefi")]
use uefi::fs::{FileSystem, Path};
#[cfg(target_os = "uefi")]
use uefi::prelude::*;
#[cfg(target_os = "uefi")]
use uefi::{CString16, boot};

#[cfg(target_os = "uefi")]
#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    log::info!("rust-boot starting...");

    let fs = match open_esp_filesystem() {
        Ok(fs) => fs,
        Err(error) => return log_boot_error("Failed to open ESP filesystem", &error),
    };
    let config = match load_config(fs) {
        Ok(config) => config,
        Err(error) => return log_status_error("Failed to load config", error),
    };
    log::info!("Loaded {} boot entries", config.entries.len());
    let selected = menu::show(&config);
    let entry = &config.entries[selected];
    log_boot_entry(entry);
    let mut fs = match open_esp_filesystem() {
        Ok(fs) => fs,
        Err(error) => return log_boot_error("Failed to reopen ESP filesystem", &error),
    };
    if let Err(error) = loader::boot_linux(&mut fs, entry) {
        return boot_failure_status(error);
    }
    Status::SUCCESS
}

#[cfg(not(target_os = "uefi"))]
fn main() {}

#[cfg(target_os = "uefi")]
fn log_boot_error<T: core::fmt::Debug>(message: &str, error: &T) -> Status {
    log::error!("{}: {:?}", message, error);
    Status::LOAD_ERROR
}

#[cfg(target_os = "uefi")]
fn log_status_error(message: &str, error: &str) -> Status {
    log::error!("{}: {}", message, error);
    Status::LOAD_ERROR
}

#[cfg(target_os = "uefi")]
fn log_boot_entry(entry: &config::BootEntry) {
    log::info!("Booting: {}", entry.title);
    log::info!("  kernel: {}", entry.kernel);
    for initrd in &entry.initrd {
        log::info!("  initrd: {}", initrd);
    }
    log::info!("  options: {}", entry.options);
}

#[cfg(target_os = "uefi")]
fn boot_failure_status(error: &str) -> Status {
    log::error!("Boot failed: {}", error);
    log::info!("Press any key to continue...");
    boot::stall(Duration::from_secs(10));
    Status::LOAD_ERROR
}

#[cfg(target_os = "uefi")]
fn open_esp_filesystem() -> uefi::Result<FileSystem> {
    let handle = boot::image_handle();
    let fs_proto = boot::get_image_file_system(handle)?;
    Ok(FileSystem::new(fs_proto))
}

#[cfg(target_os = "uefi")]
fn load_config(mut fs: FileSystem) -> Result<BootConfig, &'static str> {
    let path = CString16::try_from("\\boot.conf").map_err(|_| "Invalid path")?;
    let content = fs
        .read_to_string(Path::new(&path))
        .map_err(|_| "Failed to read boot.conf")?;

    config::parse(&content)
}
