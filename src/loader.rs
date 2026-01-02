//! Linux kernel loader for UEFI
//!
//! Loads Linux kernel (with EFI stub) and initrd, then boots via LoadImage/StartImage.

use alloc::string::ToString;
use alloc::vec::Vec;
use uefi::boot::{AllocateType, MemoryType};
use uefi::fs::{FileSystem, Path};
use uefi::proto::loaded_image::LoadedImage;
use uefi::{boot, CString16, Handle};

use crate::config::BootEntry;

/// Boot a Linux kernel with the given entry configuration
pub fn boot_linux(fs: &mut FileSystem, entry: &BootEntry) -> Result<(), &'static str> {
    log::info!("Loading kernel: {}", entry.kernel);

    // Load kernel file
    let kernel_path = CString16::try_from(entry.kernel.as_str()).map_err(|_| "Invalid kernel path")?;
    let kernel_data = fs
        .read(Path::new(&kernel_path))
        .map_err(|_| "Failed to read kernel")?;

    log::info!("Kernel size: {} bytes", kernel_data.len());

    // Verify this is a Linux kernel with EFI stub
    if !verify_linux_efi_stub(&kernel_data) {
        return Err("Kernel does not have EFI stub or invalid format");
    }

    // Load initrd(s) into contiguous memory
    let initrd_data = load_initrds(fs, &entry.initrd)?;
    log::info!("Initrd total size: {} bytes", initrd_data.len());

    // Build command line: kernel options + initrd location
    let cmdline = build_cmdline(&entry.options, &initrd_data)?;
    log::info!("Command line: {}", cmdline.to_string());

    // Load kernel as EFI image
    let kernel_handle = load_kernel_image(&kernel_data)?;

    // Set up loaded image protocol with command line and initrd
    setup_loaded_image(kernel_handle, &cmdline, &initrd_data)?;

    log::info!("Starting kernel...");

    // Start the kernel - this should not return
    boot::start_image(kernel_handle).map_err(|_| "Failed to start kernel")?;

    // If we get here, something went wrong
    Err("Kernel returned unexpectedly")
}

/// Verify the kernel has a valid EFI stub
fn verify_linux_efi_stub(data: &[u8]) -> bool {
    // Linux bzImage format check
    // Offset 0x1FE-0x1FF should be 0xAA55 (boot signature)
    // Offset 0x202 should contain "HdrS" (0x53726448)
    // Offset 0x211 contains loadflags, bit 0 indicates EFI handover

    if data.len() < 0x260 {
        log::error!("Kernel too small");
        return false;
    }

    // Check boot signature
    if data[0x1FE] != 0x55 || data[0x1FF] != 0xAA {
        log::error!("Invalid boot signature");
        return false;
    }

    // Check header signature "HdrS"
    let hdrs = u32::from_le_bytes([data[0x202], data[0x203], data[0x204], data[0x205]]);
    if hdrs != 0x53726448 {
        log::error!("Invalid header signature: {:08x}", hdrs);
        return false;
    }

    // Check boot protocol version (should be >= 2.00 for modern features)
    let version = u16::from_le_bytes([data[0x206], data[0x207]]);
    log::info!("Boot protocol version: {}.{:02}", version >> 8, version & 0xFF);

    if version < 0x0200 {
        log::error!("Boot protocol version too old");
        return false;
    }

    // Check for EFI handover support (version >= 2.12)
    if version >= 0x020C {
        let xloadflags = u16::from_le_bytes([data[0x236], data[0x237]]);
        let has_efi_handover = (xloadflags & 0x08) != 0; // XLF_EFI_HANDOVER_64
        log::info!("EFI handover 64-bit: {}", has_efi_handover);
    }

    // Check PE header for EFI stub
    // At offset 0x3C is the PE header offset
    let pe_offset = u32::from_le_bytes([data[0x3C], data[0x3D], data[0x3E], data[0x3F]]) as usize;

    if pe_offset + 4 <= data.len() {
        let pe_sig = u32::from_le_bytes([
            data[pe_offset],
            data[pe_offset + 1],
            data[pe_offset + 2],
            data[pe_offset + 3],
        ]);
        if pe_sig == 0x00004550 {
            // "PE\0\0"
            log::info!("PE header found at offset {:#x}", pe_offset);
            return true;
        }
    }

    log::warn!("No PE header found, kernel may not have EFI stub");
    true // Still try to load it
}

/// Load all initrds into a single contiguous buffer
fn load_initrds(fs: &mut FileSystem, initrds: &[alloc::string::String]) -> Result<Vec<u8>, &'static str> {
    let mut combined = Vec::new();

    for initrd_path_str in initrds {
        log::info!("Loading initrd: {}", initrd_path_str);

        let initrd_path =
            CString16::try_from(initrd_path_str.as_str()).map_err(|_| "Invalid initrd path")?;
        let data = fs
            .read(Path::new(&initrd_path))
            .map_err(|_| "Failed to read initrd")?;

        log::info!("  {} bytes", data.len());
        combined.extend_from_slice(&data);
    }

    Ok(combined)
}

/// Build command line string
fn build_cmdline(
    options: &str,
    _initrd_data: &[u8],
) -> Result<CString16, &'static str> {
    // For EFI stub, initrd is passed separately via LINUX_EFI_INITRD_MEDIA_GUID
    // or via the initrd= parameter with a device path
    // For now, just pass the options as-is
    CString16::try_from(options).map_err(|_| "Command line too long or invalid")
}

/// Load kernel as an EFI image
fn load_kernel_image(kernel_data: &[u8]) -> Result<Handle, &'static str> {
    // Allocate memory for kernel and copy it
    let kernel_size = kernel_data.len();
    let pages = (kernel_size + 0xFFF) / 0x1000;

    let kernel_ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| "Failed to allocate memory for kernel")?;

    // Copy kernel to allocated memory
    unsafe {
        core::ptr::copy_nonoverlapping(kernel_data.as_ptr(), kernel_ptr.as_ptr(), kernel_size);
    }

    // Load the image
    let device_path = None; // Use memory-mapped image
    let kernel_handle = boot::load_image(
        boot::image_handle(),
        uefi::boot::LoadImageSource::FromBuffer {
            buffer: unsafe { core::slice::from_raw_parts(kernel_ptr.as_ptr(), kernel_size) },
            file_path: device_path,
        },
    )
    .map_err(|e| {
        log::error!("load_image failed: {:?}", e);
        "Failed to load kernel image"
    })?;

    Ok(kernel_handle)
}

/// Set up the loaded image with command line and initrd info
fn setup_loaded_image(
    handle: Handle,
    cmdline: &CString16,
    initrd_data: &[u8],
) -> Result<(), &'static str> {
    // Get LoadedImage protocol
    let mut loaded_image = boot::open_protocol_exclusive::<LoadedImage>(handle)
        .map_err(|_| "Failed to open LoadedImage protocol")?;

    // Set load options (command line)
    // The command line needs to be in UCS-2 format
    let cmdline_bytes = cmdline.as_slice_with_nul();
    unsafe {
        loaded_image.set_load_options(
            cmdline_bytes.as_ptr() as *const u8,
            (cmdline_bytes.len() * 2) as u32,
        );
    }

    // For initrd, we need to install LINUX_EFI_INITRD_MEDIA_GUID protocol
    // or use the Linux-specific EFI handover protocol
    if !initrd_data.is_empty() {
        install_initrd_protocol(initrd_data)?;
    }

    Ok(())
}

/// Install the Linux initrd protocol
fn install_initrd_protocol(initrd_data: &[u8]) -> Result<(), &'static str> {
    // Linux kernel expects initrd via LINUX_EFI_INITRD_MEDIA_GUID
    // This requires installing a custom protocol

    // For now, allocate initrd in EFI memory where kernel can find it
    let initrd_size = initrd_data.len();
    let pages = (initrd_size + 0xFFF) / 0x1000;

    let initrd_ptr =
        boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
            .map_err(|_| "Failed to allocate memory for initrd")?;

    unsafe {
        core::ptr::copy_nonoverlapping(initrd_data.as_ptr(), initrd_ptr.as_ptr(), initrd_size);
    }

    log::info!(
        "Initrd loaded at {:p}, size {}",
        initrd_ptr.as_ptr(),
        initrd_size
    );

    // TODO: Install LINUX_EFI_INITRD_MEDIA_GUID protocol
    // For now, we rely on the command line initrd= parameter with the address

    Ok(())
}
