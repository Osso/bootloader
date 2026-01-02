# rust-boot: Minimal UEFI Bootloader

## Overview

A minimal UEFI bootloader written in Rust with BTRFS support. Reads boot entries from a plain text config file and presents a simple menu.

## System Context

- **Boot mode**: UEFI
- **Root filesystem**: BTRFS with subvolumes
- **Current subvolume**: `@arch`
- **Initramfs**: Required (system uses `initramfs-linux.img`)
- **Microcode**: AMD (`amd-ucode.img`)

## Features

### In Scope
- UEFI boot (x86_64)
- BTRFS filesystem reading (read-only)
- Subvolume support
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
- Other filesystems (ext4, xfs, etc.)
- Network boot
- Multiple disk support (single root device)

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     UEFI Firmware                       │
└─────────────────────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   rust-boot.efi                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
│  │ Config      │  │ BTRFS       │  │ Boot Menu       │  │
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

## Config File Format

Location: `/boot/boot.conf` (on BTRFS root or EFI partition)

```ini
# rust-boot configuration
timeout = 5
default = arch

[arch]
title = Arch Linux
kernel = /@arch/boot/vmlinuz-linux
initrd = /@arch/boot/amd-ucode.img
initrd = /@arch/boot/initramfs-linux.img
options = root=UUID=ec111172-5d9f-4fe1-98f2-2d9a2602691c rw rootflags=subvol=@arch quiet

[arch-fallback]
title = Arch Linux (fallback)
kernel = /@arch/boot/vmlinuz-linux
initrd = /@arch/boot/amd-ucode.img
initrd = /@arch/boot/initramfs-linux-fallback.img
options = root=UUID=ec111172-5d9f-4fe1-98f2-2d9a2602691c rw rootflags=subvol=@arch
```

## Module Structure

```
src/
├── main.rs           # Entry point, UEFI setup
├── config.rs         # Config file parser
├── menu.rs           # Boot menu display and selection
├── loader.rs         # Kernel/initramfs loading
└── btrfs/
    ├── mod.rs        # BTRFS module root
    ├── superblock.rs # Superblock parsing
    ├── tree.rs       # B-tree traversal
    ├── inode.rs      # Inode/extent reading
    └── subvol.rs     # Subvolume handling
```

## BTRFS Implementation

Minimal read-only implementation:

1. **Superblock**: Parse at offset 0x10000 (64KB)
2. **Chunk tree**: Map logical to physical addresses
3. **Root tree**: Find filesystem tree root
4. **Subvolume**: Resolve subvolume path to tree ID
5. **File lookup**: B-tree search for directory entries
6. **File read**: Follow extent data to read file contents

Key structures:
- Superblock (at known offset)
- Chunk items (logical→physical mapping)
- Root items (tree roots)
- Dir items (directory entries)
- Inode items (file metadata)
- Extent data (file contents)

## Boot Sequence

1. UEFI loads `rust-boot.efi` from EFI System Partition
2. Initialize UEFI protocols (SimpleTextOutput, SimpleFileSystem, BlockIO)
3. Locate BTRFS partition by UUID or scanning
4. Mount BTRFS (read superblock, chunk tree)
5. Read `/boot/boot.conf` from BTRFS
6. Parse config, build boot entry list
7. Display menu, wait for selection or timeout
8. Load selected kernel into memory
9. Load and concatenate initrd images
10. Set up Linux boot protocol structures
11. Call ExitBootServices()
12. Jump to kernel entry point

## Memory Layout

```
0x0010_0000 (1MB)     - Kernel load address (Linux default)
0x????_????           - Initramfs (after kernel, aligned)
0x????_????           - Boot params / command line
```

## Error Handling

- Config parse errors: Show error, fall back to first bootable entry
- File not found: Skip entry, warn in menu
- BTRFS read error: Fatal, display error and halt
- No bootable entries: Fatal, display error and halt

## Build & Install

```bash
# Build
cargo build --release --target x86_64-unknown-uefi

# Install (as root)
cp target/x86_64-unknown-uefi/release/rust-boot.efi /boot/efi/EFI/rust-boot/
efibootmgr -c -d /dev/nvme0n1 -p 1 -L "rust-boot" -l '\EFI\rust-boot\rust-boot.efi'
```

## Testing Strategy

1. **Unit tests**: Config parser, BTRFS structures (with mock data)
2. **Integration**: QEMU with OVMF firmware and BTRFS disk image
3. **Hardware**: Test on real hardware after QEMU validation

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| BTRFS complexity | Start with simple layouts, no RAID/compression |
| Brick system | Keep GRUB as fallback, test in QEMU first |
| UEFI quirks | Test on multiple firmware versions |

## Future Considerations (not implementing now)

- Secure Boot support
- ext4/xfs support
- Boot entry editing
- Graphical menu
- Network boot
