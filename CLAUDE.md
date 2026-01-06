# rust-boot Development

Minimal UEFI bootloader in Rust. Uses LoadImage/StartImage with LoadFile2 protocol for initrd.

## Build

```bash
RUSTC_BOOTSTRAP=1 cargo build --release -Z build-std=core,alloc --target x86_64-unknown-uefi
```

Output: `target/x86_64-unknown-uefi/release/rust-boot.efi`

## Test in QEMU

```bash
./run-tests.sh
```

Uses OVMF firmware, creates dummy kernel/initrd for menu testing. Press keys to navigate menu, Enter to select (will fail with dummy files).

Exit QEMU: `Ctrl-A X`

## Test on Real Hardware

### Setup

1. Mount ESP: `sudo mount /dev/nvme0n1p1 /mnt`
2. Copy bootloader: `sudo cp target/x86_64-unknown-uefi/release/rust-boot.efi /mnt/EFI/BOOT/BOOTX64.EFI`
3. Ensure `/mnt/boot.conf` has correct paths and options
4. Reboot and select from UEFI boot menu (F12 or similar)

### Current ESP Layout

```
/mnt/
├── boot.conf
├── arch/
│   ├── vmlinuz-linux
│   ├── amd-ucode.img
│   └── initramfs-linux.img
└── EFI/BOOT/BOOTX64.EFI
```

### Debugging Boot Issues

If something doesn't work after boot, compare with GRUB:

```bash
# Check command line was passed correctly
cat /proc/cmdline

# Check initrd loaded (should show modules)
lsmod | wc -l

# Check for I2C/ACPI/device issues
dmesg | grep -iE 'i2c|hid|acpi|touch'
```

Add `loglevel=7` and remove `quiet` from options for verbose boot output.

## Unit Tests

```bash
cargo test
```

Tests config parser only (doesn't require UEFI target).

## Architecture

- `main.rs` - UEFI entry, orchestrates boot flow
- `config.rs` - INI-style config parser
- `menu.rs` - Text-mode boot menu
- `loader.rs` - Kernel loading via LoadImage, initrd via LoadFile2 protocol

## Key Implementation Details

- Kernel loaded via `boot::load_image()` / `boot::start_image()` (uses kernel's EFI stub)
- Initrd passed via `LINUX_EFI_INITRD_MEDIA_GUID` LoadFile2 protocol
- Multiple initrds concatenated (microcode first)
- Command line passed via `LoadedImage.set_load_options()`
