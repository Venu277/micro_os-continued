<#
.SYNOPSIS
    MicroOS Automated Packaging Script: Source Code to Bootable VMDK and ISO.

.DESCRIPTION
    Builds the MicroOS kernel binary, embeds the bootloader into a bootable MBR raw disk image
    via `cargo bootimage`, generates VMware-compatible VMDK disks (`micro_os.vmdk` and `data.vmdk`),
    and attempts El Torito ISO generation via WSL `xorriso`.
#>

[CmdletBinding()]
param (
    [switch]$Release
)

$ErrorActionPreference = "Stop"

Write-Host "==================================================" -ForegroundColor Cyan
Write-Host "   MicroOS Build & Packaging Pipeline" -ForegroundColor Cyan
Write-Host "==================================================" -ForegroundColor Cyan

$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
Set-Location $WorkspaceRoot

$BuildDir = Join-Path $WorkspaceRoot "build"
if (-not (Test-Path $BuildDir)) {
    New-Item -ItemType Directory -Path $BuildDir | Out-Null
}

# 1. Check prerequisites
$QemuPath = "C:\Program Files\qemu"
if (Test-Path $QemuPath) {
    $env:PATH = "$QemuPath;$env:PATH"
}

$QemuImg = Get-Command "qemu-img" -ErrorAction SilentlyContinue
if (-not $QemuImg) {
    Write-Error "qemu-img not found in PATH or at '$QemuPath'. Please ensure QEMU is installed."
    exit 1
}

# 2. Build kernel and bootimage
Write-Host "`n[1/4] Compiling kernel and generating bootable MBR raw image..." -ForegroundColor Yellow
$BootimageArgs = @("bootimage")
if ($Release) {
    $BootimageArgs += "--release"
}
& cargo @BootimageArgs
if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo bootimage failed."
    exit $LASTEXITCODE
}

$TargetSubdir = if ($Release) { "release" } else { "debug" }
$RawImage = Join-Path $WorkspaceRoot "target\x86_64-micro_os\$TargetSubdir\bootimage-micro_os.bin"

if (-not (Test-Path $RawImage)) {
    Write-Error "Bootimage binary not found at '$RawImage'."
    exit 1
}
Write-Host " -> Bootimage created: $RawImage ($((Get-Item $RawImage).Length) bytes)" -ForegroundColor Green

# 3. Ensure 4 MiB data disk exists
$DataDisk = Join-Path $WorkspaceRoot "disk.img"
if (-not (Test-Path $DataDisk)) {
    Write-Host "`nCreating 4 MiB data disk ($DataDisk)..." -ForegroundColor Yellow
    $diskBytes = New-Object byte[] (4 * 1024 * 1024)
    [System.IO.File]::WriteAllBytes($DataDisk, $diskBytes)
}

# 4. Convert raw images to VMware VMDK format
Write-Host "`n[2/4] Converting raw images to VMware VMDK format using qemu-img..." -ForegroundColor Yellow
$KernelVmdk = Join-Path $BuildDir "micro_os.vmdk"
$DataVmdk = Join-Path $BuildDir "data.vmdk"

& qemu-img convert -f raw -O vmdk $RawImage $KernelVmdk
Write-Host " -> Kernel VMDK: $KernelVmdk ($((Get-Item $KernelVmdk).Length) bytes)" -ForegroundColor Green

& qemu-img convert -f raw -O vmdk $DataDisk $DataVmdk
Write-Host " -> Data Disk VMDK: $DataVmdk ($((Get-Item $DataVmdk).Length) bytes)" -ForegroundColor Green

# 5. Generate El Torito ISO via WSL xorriso (if available)
Write-Host "`n[3/4] Generating El Torito ISO image via xorriso..." -ForegroundColor Yellow
$WslCmd = Get-Command "wsl" -ErrorAction SilentlyContinue
$IsoPath = Join-Path $BuildDir "micro_os.iso"

if ($WslCmd) {
    try {
        $WslScript = "mkdir -p /tmp/micro_os_iso && cp `$(wslpath -u '$RawImage') /tmp/micro_os_iso/boot.img && xorriso -as mkisofs -R -J -c boot.catalog -b boot.img -hard-disk-boot -o `$(wslpath -u '$IsoPath') /tmp/micro_os_iso 2>&1"
        $wslOut = wsl bash -c "$WslScript"
        if (Test-Path $IsoPath) {
            Write-Host " -> Bootable ISO: $IsoPath ($((Get-Item $IsoPath).Length) bytes)" -ForegroundColor Green
        } else {
            Write-Warning "ISO generation did not produce output: $wslOut"
        }
    } catch {
        Write-Warning "WSL xorriso invocation encountered an error: $_"
    }
} else {
    Write-Host " -> WSL not detected; skipping ISO generation. VMDK is the primary format for VMware." -ForegroundColor DarkGray
}

# 6. Summary of packaged artifacts
Write-Host "`n[4/4] Packaging complete! Artifacts in '$BuildDir':" -ForegroundColor Cyan
Get-ChildItem -Path $BuildDir | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize

Write-Host "To boot the VMDK in QEMU: .\scripts\run_vmdk.ps1" -ForegroundColor Yellow
Write-Host "To boot in VMware Workstation: open 'vmware\micro_os.vmx'" -ForegroundColor Yellow
