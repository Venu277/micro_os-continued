<#
.SYNOPSIS
    Boot the packaged MicroOS VMDK disk images in QEMU.
#>

$WorkspaceRoot = Split-Path -Parent $PSScriptRoot
Set-Location $WorkspaceRoot

$QemuPath = "C:\Program Files\qemu"
if (Test-Path $QemuPath) {
    $env:PATH = "$QemuPath;$env:PATH"
}

$KernelVmdk = Join-Path $WorkspaceRoot "build\micro_os.vmdk"
$DataVmdk = Join-Path $WorkspaceRoot "build\data.vmdk"

if (-not (Test-Path $KernelVmdk)) {
    Write-Host "VMDK not found. Running .\scripts\package.ps1 first..." -ForegroundColor Yellow
    & "$PSScriptRoot\package.ps1"
}

Write-Host "Booting MicroOS VMDK in QEMU (IDE Master: micro_os.vmdk, IDE Slave: data.vmdk)..." -ForegroundColor Cyan
& qemu-system-x86_64 `
    -drive format=vmdk,file=$KernelVmdk,if=ide,index=0,media=disk `
    -drive format=vmdk,file=$DataVmdk,if=ide,index=1,media=disk `
    -serial stdio `
    -m 512M
