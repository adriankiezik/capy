# capy-fsr-setup.ps1 - One-time FSR (FidelityFX SDK) environment setup for capy-project

# 1. FidelityFX SDK (clone if not already present)
$FsrSdkPath = 'C:\SDKs\FidelityFX-SDK-2.2.0'
if (-not (Test-Path $FsrSdkPath)) {
    Write-Host 'Cloning AMD FidelityFX SDK v2.2.0 (this may take a while)...' -ForegroundColor Cyan
    git clone --branch v2.2.0 --depth 1 https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK $FsrSdkPath
    if ($LASTEXITCODE -ne 0) {
        Write-Host 'Failed to clone FidelityFX SDK.' -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "FidelityFX SDK already exists at $FsrSdkPath" -ForegroundColor Green
}

# 2. Verify expected SDK structure
$ApiHeader = Join-Path $FsrSdkPath 'Kits\FidelityFX\api\include\ffx_api.h'
$Dx12Header = Join-Path $FsrSdkPath 'Kits\FidelityFX\api\include\dx12\ffx_api_dx12.h'
$LoaderLib = Join-Path $FsrSdkPath 'Kits\FidelityFX\signedbin\amd_fidelityfx_loader_dx12.lib'

$MissingFiles = @()
if (-not (Test-Path $ApiHeader)) { $MissingFiles += $ApiHeader }
if (-not (Test-Path $Dx12Header)) { $MissingFiles += $Dx12Header }
if (-not (Test-Path $LoaderLib)) { $MissingFiles += $LoaderLib }

if ($MissingFiles.Count -gt 0) {
    Write-Host 'SDK structure validation failed. Missing files:' -ForegroundColor Red
    foreach ($f in $MissingFiles) {
        Write-Host "  - $f" -ForegroundColor Red
    }
    exit 1
}
Write-Host 'SDK structure validated.' -ForegroundColor Green

# 3. Set FSR_SDK environment variable
$env:FSR_SDK = $FsrSdkPath
[Environment]::SetEnvironmentVariable('FSR_SDK', $FsrSdkPath, 'User')
Write-Host "FSR_SDK = $env:FSR_SDK" -ForegroundColor Green

# 4. Summary
Write-Host ''
Write-Host '--- Environment ready ---' -ForegroundColor Cyan
Write-Host "FSR_SDK = $env:FSR_SDK"
Write-Host ''
Write-Host 'SDK layout:' -ForegroundColor Cyan
Write-Host "  Headers:  $FsrSdkPath\Kits\FidelityFX\api\include\"
Write-Host "  DX12:     $FsrSdkPath\Kits\FidelityFX\api\include\dx12\"
Write-Host "  Upscaler: $FsrSdkPath\Kits\FidelityFX\upscalers\include\"
Write-Host "  FrameGen: $FsrSdkPath\Kits\FidelityFX\framegeneration\include\"
Write-Host "  Libs:     $FsrSdkPath\Kits\FidelityFX\signedbin\"
Write-Host ''
Write-Host 'Runtime DLLs (copy to target dir or keep in PATH):' -ForegroundColor Cyan
Write-Host "  amd_fidelityfx_loader_dx12.dll"
Write-Host "  amd_fidelityfx_upscaler_dx12.dll"
Write-Host "  amd_fidelityfx_framegeneration_dx12.dll"
Write-Host ''
Write-Host 'Next: open a NEW terminal, then run:' -ForegroundColor Yellow
Write-Host '  cargo check -p capy_game --features fsr' -ForegroundColor White
