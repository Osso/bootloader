//! Linux kernel loader for UEFI
//!
//! Loads Linux kernel (with EFI stub) and initrd, then boots via LoadImage/StartImage.
//! Implements LINUX_EFI_INITRD_MEDIA_GUID protocol for proper initrd passing.

use alloc::string::ToString;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::ptr;
use uefi::boot::{AllocateType, MemoryType};
use uefi::fs::{FileSystem, Path};
use uefi::proto::loaded_image::LoadedImage;
use uefi::{CString16, Guid, Handle, Status, boot, guid};

use crate::config::BootEntry;

/// LINUX_EFI_INITRD_MEDIA_GUID - used by Linux kernel to find initrd
const LINUX_EFI_INITRD_MEDIA_GUID: Guid = guid!("5568e427-68fc-4f3d-ac74-ca555231cc68");

/// LoadFile2 protocol GUID
const EFI_LOAD_FILE2_PROTOCOL_GUID: Guid = guid!("4006c0c1-fcb3-403e-996d-4a6c8724e06d");

/// Device Path protocol GUID
const EFI_DEVICE_PATH_PROTOCOL_GUID: Guid = guid!("09576e91-6d3f-11d2-8e39-00a0c969723b");

const PAGE_SIZE: usize = 0x1000;
const PAGE_ALIGN_MASK: usize = PAGE_SIZE - 1;
const MIN_KERNEL_STUB_SIZE: usize = 0x260;
const BOOT_SIGNATURE_OFFSET: usize = 0x1FE;
const SETUP_HEADER_SIGNATURE_OFFSET: usize = 0x202;
const BOOT_PROTOCOL_VERSION_OFFSET: usize = 0x206;
const MIN_BOOT_PROTOCOL_VERSION: u16 = 0x0200;
const EFI_HANDOVER_VERSION: u16 = 0x020C;
const XLOADFLAGS_OFFSET: usize = 0x236;
const PE_HEADER_POINTER_OFFSET: usize = 0x3C;

const BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];
const SETUP_HEADER_SIGNATURE: u32 = 0x5372_6448;
const EFI_HANDOVER_64_FLAG: u16 = 0x08;
const PE_SIGNATURE: u32 = 0x0000_4550;

/// Global initrd data - must remain valid until kernel takes over
/// Using static mut because UEFI callbacks need access to this
static mut INITRD_DATA: Option<InitrdInfo> = None;

struct InitrdInfo {
    data: *const u8,
    size: usize,
}

/// LoadFile2 protocol structure
#[repr(C)]
struct LoadFile2Protocol {
    load_file: unsafe extern "efiapi" fn(
        this: *const LoadFile2Protocol,
        file_path: *const c_void,
        boot_policy: bool,
        buffer_size: *mut usize,
        buffer: *mut c_void,
    ) -> Status,
}

/// Vendor device path for initrd
#[repr(C, packed)]
struct InitrdDevicePath {
    vendor: VendorDevicePath,
    end: EndDevicePath,
}

#[repr(C, packed)]
struct VendorDevicePath {
    header: DevicePathHeader,
    guid: Guid,
}

#[repr(C, packed)]
struct EndDevicePath {
    header: DevicePathHeader,
}

#[repr(C, packed)]
struct DevicePathHeader {
    device_type: u8,
    sub_type: u8,
    length: [u8; 2],
}

/// Static protocol instance
static LOAD_FILE2_PROTOCOL: LoadFile2Protocol = LoadFile2Protocol {
    load_file: initrd_load_file,
};

/// Static device path for initrd
static INITRD_DEVICE_PATH: InitrdDevicePath = InitrdDevicePath {
    vendor: VendorDevicePath {
        header: DevicePathHeader {
            device_type: 0x04, // MEDIA_DEVICE_PATH
            sub_type: 0x03,    // MEDIA_VENDOR_DP
            length: [20, 0],   // sizeof(VendorDevicePath) = 20
        },
        guid: LINUX_EFI_INITRD_MEDIA_GUID,
    },
    end: EndDevicePath {
        header: DevicePathHeader {
            device_type: 0x7F, // END_DEVICE_PATH_TYPE
            sub_type: 0xFF,    // END_ENTIRE_DEVICE_PATH_SUBTYPE
            length: [4, 0],    // sizeof(EndDevicePath) = 4
        },
    },
};

/// LoadFile2 callback - called by Linux kernel to get initrd
unsafe extern "efiapi" fn initrd_load_file(
    _this: *const LoadFile2Protocol,
    _file_path: *const c_void,
    boot_policy: bool,
    buffer_size: *mut usize,
    buffer: *mut c_void,
) -> Status {
    // Boot policy must be false for LoadFile2
    if boot_policy {
        return Status::UNSUPPORTED;
    }

    let initrd_ptr = &raw const INITRD_DATA;
    let initrd = match unsafe { &*initrd_ptr } {
        Some(info) => info,
        None => return Status::NOT_FOUND,
    };

    if buffer_size.is_null() {
        return Status::INVALID_PARAMETER;
    }

    let required_size = initrd.size;

    // If buffer is null or too small, return required size
    if buffer.is_null() || unsafe { *buffer_size } < required_size {
        unsafe { *buffer_size = required_size };
        return Status::BUFFER_TOO_SMALL;
    }

    // Copy initrd to provided buffer
    unsafe {
        ptr::copy_nonoverlapping(initrd.data, buffer as *mut u8, required_size);
        *buffer_size = required_size;
    }

    Status::SUCCESS
}

/// Boot a Linux kernel with the given entry configuration
pub fn boot_linux(fs: &mut FileSystem, entry: &BootEntry) -> Result<(), &'static str> {
    log::info!("Loading kernel: {}", entry.kernel);

    // Load kernel file
    let kernel_path =
        CString16::try_from(entry.kernel.as_str()).map_err(|_| "Invalid kernel path")?;
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

    // Install initrd protocol BEFORE loading kernel
    if !initrd_data.is_empty() {
        install_initrd_protocol(&initrd_data)?;
    }

    // Build command line
    let cmdline = build_cmdline(&entry.options)?;
    log::info!("Command line: {}", cmdline.to_string());

    // Load kernel as EFI image
    let kernel_handle = load_kernel_image(&kernel_data)?;

    // Set up loaded image protocol with command line
    setup_loaded_image(kernel_handle, &cmdline)?;

    log::info!("Starting kernel...");

    // Start the kernel - this should not return
    boot::start_image(kernel_handle).map_err(|_| "Failed to start kernel")?;

    // If we get here, something went wrong
    Err("Kernel returned unexpectedly")
}

/// Verify the kernel has a valid EFI stub
fn verify_linux_efi_stub(data: &[u8]) -> bool {
    if data.len() < MIN_KERNEL_STUB_SIZE {
        log::error!("Kernel too small");
        return false;
    }

    if read_bytes::<2>(data, BOOT_SIGNATURE_OFFSET) != Some(BOOT_SIGNATURE) {
        log::error!("Invalid boot signature");
        return false;
    }

    let hdrs = read_u32(data, SETUP_HEADER_SIGNATURE_OFFSET).unwrap_or_default();
    if hdrs != SETUP_HEADER_SIGNATURE {
        log::error!("Invalid header signature: {:08x}", hdrs);
        return false;
    }

    let version = read_u16(data, BOOT_PROTOCOL_VERSION_OFFSET).unwrap_or_default();
    log::info!(
        "Boot protocol version: {}.{:02}",
        version >> 8,
        version & 0xFF
    );

    if version < MIN_BOOT_PROTOCOL_VERSION {
        log::error!("Boot protocol version too old");
        return false;
    }

    if version >= EFI_HANDOVER_VERSION {
        let xloadflags = read_u16(data, XLOADFLAGS_OFFSET).unwrap_or_default();
        let has_efi_handover = (xloadflags & EFI_HANDOVER_64_FLAG) != 0;
        log::info!("EFI handover 64-bit: {}", has_efi_handover);
    }

    let pe_offset = read_u32(data, PE_HEADER_POINTER_OFFSET).unwrap_or_default() as usize;
    if pe_offset + 4 <= data.len() {
        let pe_sig = read_u32(data, pe_offset).unwrap_or_default();
        if pe_sig == PE_SIGNATURE {
            log::info!("PE header found at offset {:#x}", pe_offset);
            return true;
        }
    }

    log::warn!("No PE header found, kernel may not have EFI stub");
    true
}

/// Load all initrds into a single contiguous buffer
fn load_initrds(
    fs: &mut FileSystem,
    initrds: &[alloc::string::String],
) -> Result<Vec<u8>, &'static str> {
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
fn build_cmdline(options: &str) -> Result<CString16, &'static str> {
    CString16::try_from(options).map_err(|_| "Command line too long or invalid")
}

/// Load kernel as an EFI image
fn load_kernel_image(kernel_data: &[u8]) -> Result<Handle, &'static str> {
    let kernel_size = kernel_data.len();
    let pages = pages_for_size(kernel_size);

    let kernel_ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| "Failed to allocate memory for kernel")?;

    unsafe {
        ptr::copy_nonoverlapping(kernel_data.as_ptr(), kernel_ptr.as_ptr(), kernel_size);
    }

    let kernel_handle = boot::load_image(
        boot::image_handle(),
        uefi::boot::LoadImageSource::FromBuffer {
            buffer: unsafe { core::slice::from_raw_parts(kernel_ptr.as_ptr(), kernel_size) },
            file_path: None,
        },
    )
    .map_err(|e| {
        log::error!("load_image failed: {:?}", e);
        "Failed to load kernel image"
    })?;

    Ok(kernel_handle)
}

/// Set up the loaded image with command line
fn setup_loaded_image(handle: Handle, cmdline: &CString16) -> Result<(), &'static str> {
    let mut loaded_image = boot::open_protocol_exclusive::<LoadedImage>(handle)
        .map_err(|_| "Failed to open LoadedImage protocol")?;

    let cmdline_bytes = cmdline.as_slice_with_nul();
    unsafe {
        loaded_image.set_load_options(
            cmdline_bytes.as_ptr() as *const u8,
            (cmdline_bytes.len() * 2) as u32,
        );
    }

    Ok(())
}

/// Install the LINUX_EFI_INITRD_MEDIA_GUID LoadFile2 protocol
fn install_initrd_protocol(initrd_data: &[u8]) -> Result<(), &'static str> {
    let initrd_size = initrd_data.len();
    let pages = pages_for_size(initrd_size);

    let initrd_ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
        .map_err(|_| "Failed to allocate memory for initrd")?;

    unsafe {
        ptr::copy_nonoverlapping(initrd_data.as_ptr(), initrd_ptr.as_ptr(), initrd_size);

        // Store initrd info globally for the LoadFile2 callback
        INITRD_DATA = Some(InitrdInfo {
            data: initrd_ptr.as_ptr(),
            size: initrd_size,
        });
    }

    log::info!("Initrd at {:p}, size {}", initrd_ptr.as_ptr(), initrd_size);

    let status = install_load_file2_protocol(boot_services()?);

    if status.is_success() {
        log::info!("LoadFile2 initrd protocol installed");
        Ok(())
    } else {
        log::error!("Failed to install initrd protocol: {:?}", status);
        Err("Failed to install initrd protocol")
    }
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(read_bytes::<2>(data, offset)?))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(read_bytes::<4>(data, offset)?))
}

fn read_bytes<const N: usize>(data: &[u8], offset: usize) -> Option<[u8; N]> {
    let end = offset.checked_add(N)?;
    let bytes = data.get(offset..end)?;
    bytes.try_into().ok()
}

fn pages_for_size(size: usize) -> usize {
    (size + PAGE_ALIGN_MASK) / PAGE_SIZE
}

fn boot_services() -> Result<*mut uefi::table::boot::BootServices, &'static str> {
    let system_table = uefi::table::system_table_raw().ok_or("Failed to get system table")?;
    let boot_services = unsafe { (*system_table.as_ptr()).boot_services };
    if boot_services.is_null() {
        return Err("Boot services not available");
    }
    Ok(boot_services)
}

fn install_load_file2_protocol(boot_services: *mut uefi::table::boot::BootServices) -> Status {
    let mut handle: *mut c_void = ptr::null_mut();
    unsafe {
        ((*boot_services).install_multiple_protocol_interfaces)(
            &mut handle,
            &EFI_LOAD_FILE2_PROTOCOL_GUID as *const Guid as *const c_void,
            &LOAD_FILE2_PROTOCOL as *const LoadFile2Protocol as *const c_void,
            &EFI_DEVICE_PATH_PROTOCOL_GUID as *const Guid as *const c_void,
            &INITRD_DEVICE_PATH as *const InitrdDevicePath as *const c_void,
            ptr::null::<c_void>(),
        )
    }
}
