# capy-dlss-setup.ps1 - One-time DLSS environment setup for capy-project

# 1. DLSS SDK (clone if not already present)
$DlssSdkPath = 'C:\SDKs\DLSS-310.5.0'
if (-not (Test-Path $DlssSdkPath)) {
    Write-Host 'Cloning NVIDIA DLSS SDK v310.5.0...' -ForegroundColor Cyan
    git clone --branch v310.5.0 --depth 1 --recurse-submodules https://github.com/NVIDIA/DLSS $DlssSdkPath
} else {
    Write-Host "DLSS SDK already exists at $DlssSdkPath" -ForegroundColor Green
}
$env:DLSS_SDK = $DlssSdkPath
[Environment]::SetEnvironmentVariable('DLSS_SDK', $DlssSdkPath, 'User')
Write-Host "DLSS_SDK = $env:DLSS_SDK" -ForegroundColor Green

# 2. Vulkan SDK (detect existing install)
$VulkanBase = 'C:\VulkanSDK'
if (Test-Path $VulkanBase) {
    $Latest = Get-ChildItem $VulkanBase -Directory | Sort-Object Name -Descending | Select-Object -First 1
    if ($Latest) {
        $env:VULKAN_SDK = $Latest.FullName
        [Environment]::SetEnvironmentVariable('VULKAN_SDK', $Latest.FullName, 'User')
        Write-Host "VULKAN_SDK = $env:VULKAN_SDK" -ForegroundColor Green
    }
} else {
    Write-Host 'Vulkan SDK not found at C:\VulkanSDK - install from https://vulkan.lunarg.com/sdk/home' -ForegroundColor Yellow
}

# 3. DLSS Project ID (generate once, reuse forever)
$Existing = [Environment]::GetEnvironmentVariable('CAPY_DLSS_PROJECT_ID', 'User')
if ($Existing) {
    $env:CAPY_DLSS_PROJECT_ID = $Existing
    Write-Host "CAPY_DLSS_PROJECT_ID = $Existing (already set)" -ForegroundColor Green
} else {
    $Id = [guid]::NewGuid().ToString()
    $env:CAPY_DLSS_PROJECT_ID = $Id
    [Environment]::SetEnvironmentVariable('CAPY_DLSS_PROJECT_ID', $Id, 'User')
    Write-Host "CAPY_DLSS_PROJECT_ID = $Id (newly generated)" -ForegroundColor Green
}

# 4. Summary
Write-Host ''
Write-Host '--- Environment ready ---' -ForegroundColor Cyan
Write-Host "DLSS_SDK              = $env:DLSS_SDK"
Write-Host "VULKAN_SDK            = $env:VULKAN_SDK"
Write-Host "CAPY_DLSS_PROJECT_ID  = $env:CAPY_DLSS_PROJECT_ID"
Write-Host ''
Write-Host 'Next: open a NEW terminal, then run:' -ForegroundColor Yellow
Write-Host '  cargo check -p capy_game --features dlss' -ForegroundColor White
