#!/usr/bin/env bash
# MicroOS Packaging Script for Linux / WSL
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_DIR="$WORKSPACE_ROOT/build"

mkdir -p "$BUILD_DIR"

echo "=================================================="
echo "   MicroOS Build & Packaging Pipeline (Linux/WSL) "
echo "=================================================="

# Check dependencies
command -v cargo >/dev/null 2>&1 || { echo "cargo not found"; exit 1; }
command -v qemu-img >/dev/null 2>&1 || { echo "qemu-img not found"; exit 1; }

echo "[1/3] Building bootimage..."
cd "$WORKSPACE_ROOT"
cargo bootimage

RAW_IMAGE="$WORKSPACE_ROOT/target/x86_64-micro_os/debug/bootimage-micro_os.bin"
DATA_DISK="$WORKSPACE_ROOT/disk.img"

if [ ! -f "$DATA_DISK" ]; then
    echo "Creating 4 MiB data disk..."
    dd if=/dev/zero of="$DATA_DISK" bs=1M count=4 status=none
fi

echo "[2/3] Converting to VMDK format..."
qemu-img convert -f raw -O vmdk "$RAW_IMAGE" "$BUILD_DIR/micro_os.vmdk"
qemu-img convert -f raw -O vmdk "$DATA_DISK" "$BUILD_DIR/data.vmdk"

echo "[3/3] Generating El Torito ISO with xorriso..."
if command -v xorriso >/dev/null 2>&1; then
    TMP_ISO_DIR="/tmp/micro_os_iso_$$"
    mkdir -p "$TMP_ISO_DIR"
    cp "$RAW_IMAGE" "$TMP_ISO_DIR/boot.img"
    xorriso -as mkisofs -R -J \
        -c boot.catalog \
        -b boot.img \
        -hard-disk-boot \
        -o "$BUILD_DIR/micro_os.iso" \
        "$TMP_ISO_DIR"
    rm -rf "$TMP_ISO_DIR"
    echo " -> ISO generated at $BUILD_DIR/micro_os.iso"
fi

echo "Packaging complete! Files in $BUILD_DIR:"
ls -lh "$BUILD_DIR"
