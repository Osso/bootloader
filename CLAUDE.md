# bootloader Development

Minimal UEFI bootloader in Rust. Uses LoadImage/StartImage with LoadFile2 protocol for initrd.

## Build

```bash
RUSTC_BOOTSTRAP=1 cargo build --release -Z build-std=core,alloc --target x86_64-unknown-uefi
```

Output: `target/x86_64-unknown-uefi/release/bootloader.efi`

Note: The menu shows numbered entries (1, 2, 3...). Press a number to boot that entry, or Enter for the default (marked with *).

## Test in QEMU

```bash
./run-tests.sh
```

Uses OVMF firmware, creates dummy kernel/initrd for menu testing. Press 1-N to select entry, Enter for default (will fail with dummy files).

Exit QEMU: `Ctrl-A X`

## Test on Real Hardware

### Setup

1. Mount ESP: `sudo mount /dev/nvme0n1p1 /mnt`
2. Copy: `sudo cp target/x86_64-unknown-uefi/release/bootloader.efi /mnt/EFI/BOOT/BOOTX64.EFI`
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
├── custom/
│   └── vmlinuz-custom      # Custom kernel (no initramfs needed)
└── EFI/BOOT/BOOTX64.EFI
```

### boot.conf Format

```ini
default = arch

[arch]
title = Arch Linux
kernel = \arch\vmlinuz-linux
initrd = \arch\amd-ucode.img
initrd = \arch\initramfs-linux.img
options = root=UUID=ec111172-5d9f-4fe1-98f2-2d9a2602691c rw rootflags=subvol=@arch quiet

[custom]
title = linux 6.19-1
kernel = \custom\vmlinuz-custom
options = root=UUID=ec111172-5d9f-4fe1-98f2-2d9a2602691c rw rootflags=subvol=@arch loglevel=7
```

### Custom Kernel Setup

The custom kernel at `/syncthing/Sync/Projects/system/linux` is built with all drivers as built-in (no modules), so it doesn't need an initramfs.

Update custom kernel on ESP after rebuild:

```bash
sudo mount /dev/nvme0n1p1 /mnt
sudo cp /syncthing/Sync/Projects/system/linux/arch/x86/boot/bzImage /mnt/custom/vmlinuz-custom
sudo umount /mnt
```

Update bootloader on ESP after rebuild:

```bash
sudo mount /dev/nvme0n1p1 /mnt
sudo cp target/x86_64-unknown-uefi/release/bootloader.efi /mnt/EFI/BOOT/BOOTX64.EFI
sudo umount /mnt
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

## Size Comparison

| Bootloader | Language | Code Lines |
|------------|----------|------------|
| bootloader | Rust | 667 |
| Sprout | Rust | 3,665 |
| rust-osdev/bootloader | Rust | 7,654 |
| Limine | C | 18,877 |
| GRUB | C | 305,161 |

Other Rust UEFI bootloaders:
- [Sprout](https://github.com/edera-dev/sprout) - Programmable, supports x86_64/aarch64/riscv64
- [rust-osdev/bootloader](https://github.com/rust-osdev/bootloader) - BIOS + UEFI, loads ELF kernels
