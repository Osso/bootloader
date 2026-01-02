# rust-boot: Minimal UEFI Bootloader

## Overview

A minimal UEFI bootloader written in Rust. Reads boot entries from a plain text config file on the EFI System Partition and presents a simple menu.

## System Context

- **Boot mode**: UEFI
- **Root filesystem**: BTRFS with subvolumes (`@arch`, `@ubuntu`)
- **ESP**: 260MB FAT32 (expandable to 2.2GB by removing unused Ubuntu-boot partition)
- **Boot files**: Stored on ESP, synced via pacman hook
- **Initramfs**: Required (system uses `initramfs-linux.img`)
- **Microcode**: AMD (`amd-ucode.img`)

## Features

### In Scope
- UEFI boot (x86_64)
- Read files from ESP via UEFI SimpleFileSystem protocol
- Plain text config file (`boot.conf`)
- Simple text-based boot menu
- Kernel loading (vmlinuz)
- Initramfs loading
- Microcode loading (prepended to initramfs)
- Kernel command line passing
- Timeout with default entry

### Out of Scope
- BIOS/Legacy boot
- Config editing at boot time
- Secure Boot signing (initially)
- Filesystem drivers (BTRFS, ext4, etc.) - uses UEFI-provided FAT32
- Network boot

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     UEFI Firmware                       │
│              (provides FAT32 filesystem)                │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   rust-boot.efi                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │ Config      │  │ File        │  │ Boot Menu       │  │
│  │ Parser      │  │ Reader      │  │ (text mode)     │  │
│  └─────────────┘  └─────────────┘  └─────────────────┘  │
│  ┌─────────────────────────────────────────────────────┐│
│  │              Kernel Loader                          ││
│  │  - Load vmlinuz                                     ││
│  │  - Load initramfs                                   ││
│  │  - Prepend microcode                                ││
│  │  - Set up boot params                               ││
│  │  - ExitBootServices & jump                          ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                    Linux Kernel                         │
└─────────────────────────────────────────────────────────┘
```

## ESP Layout

```
/boot/efi/                          # ESP mount point
├── EFI/
│   ├── rust-boot/
│   │   └── rust-boot.efi           # Bootloader
│   ├── Arch/
│   │   └── grubx64.efi             # GRUB fallback
│   └── ubuntu/
│       └── ...                     # Ubuntu (if needed)
├── boot.conf                       # Boot configuration
├── arch/
│   ├── vmlinuz-linux
│   ├── amd-ucode.img
│   └── initramfs-linux.img
└── arch-fallback/
    ├── vmlinuz-linux
    ├── amd-ucode.img
    └── initramfs-linux-fallback.img
```

## Config File Format

Location: `/boot.conf` on ESP (root of FAT32 partition)

```ini
# rust-boot configuration
timeout = 5
default = arch

[arch]
title = Arch Linux
kernel = /arch/vmlinuz-linux
initrd = /arch/amd-ucode.img
initrd = /arch/initramfs-linux.img
options = root=UUID=ec111172-5d9f-4fe1-98f2-2d9a2602691c rw rootflags=subvol=@arch quiet

[arch-fallback]
title = Arch Linux (fallback)
kernel = /arch-fallback/vmlinuz-linux
initrd = /arch-fallback/amd-ucode.img
initrd = /arch-fallback/initramfs-linux-fallback.img
options = root=UUID=ec111172-5d9f-4fe1-98f2-2d9a2602691c rw rootflags=subvol=@arch
```

## Module Structure

```
src/
├── main.rs           # Entry point, UEFI setup
├── config.rs         # Config file parser
├── menu.rs           # Boot menu display and selection
├── fs.rs             # File reading via UEFI SimpleFileSystem
└── loader.rs         # Kernel/initramfs loading, boot protocol
```

## Boot Sequence

1. UEFI loads `rust-boot.efi` from EFI System Partition
2. Initialize UEFI protocols (SimpleTextOutput, SimpleFileSystem)
3. Read `/boot.conf` from ESP
4. Parse config, build boot entry list
5. Display menu, wait for selection or timeout
6. Load selected kernel into memory
7. Load and concatenate initrd images
8. Set up Linux boot protocol structures
9. Call ExitBootServices()
10. Jump to kernel entry point

## Memory Layout

```
0x0010_0000 (1MB)     - Kernel load address (Linux default)
0x????_????           - Initramfs (after kernel, aligned)
0x????_????           - Boot params / command line
```

## Error Handling

- Config parse errors: Show error, fall back to first bootable entry
- File not found: Skip entry, warn in menu
- No bootable entries: Fatal, display error and halt

## Build & Install

```bash
# Build
cargo build --release --target x86_64-unknown-uefi

# Install (as root)
mkdir -p /boot/efi/EFI/rust-boot
cp target/x86_64-unknown-uefi/release/rust-boot.efi /boot/efi/EFI/rust-boot/
efibootmgr -c -d /dev/nvme0n1 -p 1 -L "rust-boot" -l '\EFI\rust-boot\rust-boot.efi'
```

## Pacman Hook

Sync kernel files to ESP on updates:

```ini
# /etc/pacman.d/hooks/rust-boot-sync.hook
[Trigger]
Operation = Install
Operation = Upgrade
Type = Path
Target = usr/lib/modules/*/vmlinuz
Target = boot/initramfs-*.img
Target = boot/amd-ucode.img

[Action]
Description = Syncing boot files to ESP...
When = PostTransaction
Exec = /usr/local/bin/rust-boot-sync
```

```bash
#!/bin/bash
# /usr/local/bin/rust-boot-sync
ESP=/boot/efi
mkdir -p "$ESP/arch"
cp /boot/vmlinuz-linux "$ESP/arch/"
cp /boot/amd-ucode.img "$ESP/arch/"
cp /boot/initramfs-linux.img "$ESP/arch/"
```

## Testing Strategy

1. **Unit tests**: Config parser (with mock data)
2. **Integration**: QEMU with OVMF firmware
3. **Hardware**: Test on real hardware after QEMU validation

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Brick system | Keep GRUB as fallback, test in QEMU first |
| UEFI quirks | Test on multiple firmware versions |
| ESP full | Monitor space; 260MB fits ~4 kernel sets |

## Future Considerations (not implementing now)

- Secure Boot support
- Boot entry editing
- Graphical menu
- Automatic kernel discovery (like systemd-boot)
