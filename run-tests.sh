#!/bin/bash
# Test rust-boot in QEMU with OVMF

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Build if needed
RUSTC_BOOTSTRAP=1 cargo build --release -Z build-std=core,alloc --target x86_64-unknown-uefi

# Create ESP directory structure
mkdir -p esp/EFI/BOOT

# Copy bootloader as default boot entry
cp target/x86_64-unknown-uefi/release/rust-boot.efi esp/EFI/BOOT/BOOTX64.EFI

# Create test boot.conf
cat > esp/boot.conf << 'EOF'
timeout=5
default=arch

[arch]
title=Arch Linux
kernel=\vmlinuz-linux
initrd=\initramfs-linux.img
options=root=UUID=test-uuid rw

[arch-fallback]
title=Arch Linux (fallback)
kernel=\vmlinuz-linux
initrd=\initramfs-linux-fallback.img
options=root=UUID=test-uuid rw
EOF

# Create dummy kernel/initrd (just for testing menu)
echo "dummy kernel" > esp/vmlinuz-linux
echo "dummy initrd" > esp/initramfs-linux.img
echo "dummy initrd fallback" > esp/initramfs-linux-fallback.img

# Run QEMU with OVMF
qemu-system-x86_64 \
    -enable-kvm \
    -m 256M \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.4m.fd \
    -drive if=pflash,format=raw,file=/tmp/OVMF_VARS.4m.fd \
    -drive format=raw,file=fat:rw:esp \
    -nographic \
    -serial mon:stdio
