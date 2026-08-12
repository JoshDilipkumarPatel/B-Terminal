# Enable-WindowsRSS.ps1
# -------------------------------------------------------------
# B-Terminal Hardware RSS (Receive Side Scaling) Optimizer
# Run this script as Administrator to tune your Network Adapter
# for ultra-low latency algorithmic trading.
# -------------------------------------------------------------

Requires -RunAsAdministrator

Write-Host "=============================================" -ForegroundColor Cyan
Write-Host " B-Terminal: Hardware RSS / NIC Optimizer" -ForegroundColor Cyan
Write-Host "=============================================" -ForegroundColor Cyan

# Find the primary active network adapter (connected to the internet)
$Adapter = Get-NetAdapter | Where-Object { $_.Status -eq "Up" -and $_.MacAddress -ne "" } | Select-Object -First 1

if (-not $Adapter) {
    Write-Host "[ERROR] Could not detect an active network adapter." -ForegroundColor Red
    Exit 1
}

Write-Host "[*] Active Network Adapter detected: $($Adapter.Name) ($($Adapter.InterfaceDescription))" -ForegroundColor Yellow

# Enable RSS on the adapter
Write-Host "[*] Enabling Receive Side Scaling (RSS)..." -ForegroundColor Yellow
Enable-NetAdapterRss -Name $Adapter.Name -ErrorAction SilentlyContinue

# Configure RSS CPU alignment
# We reserve Core 0 for OS interrupts, and distribute RSS queues starting from Core 1
# MaxProcessors = 4 (aligns with our 4 Tokio worker threads)

Write-Host "[*] Configuring RSS Base Processor and Max Processors..." -ForegroundColor Yellow
Set-NetAdapterRss -Name $Adapter.Name -BaseProcessorGroup 0 -BaseProcessorNumber 1 -MaxProcessors 4 -Profile NUMAStatic -ErrorAction SilentlyContinue

# Verify the changes
$RssStatus = Get-NetAdapterRss -Name $Adapter.Name
Write-Host "=============================================" -ForegroundColor Cyan
Write-Host " RSS Configuration Applied Successfully:" -ForegroundColor Green
Write-Host " Enabled           : $($RssStatus.Enabled)"
Write-Host " Base Processor    : $($RssStatus.BaseProcessorNumber)"
Write-Host " Max Processors    : $($RssStatus.MaxProcessors)"
Write-Host " Profile           : $($RssStatus.Profile)"
Write-Host "=============================================" -ForegroundColor Cyan

Write-Host "Your system is now optimized for B-Terminal hardware offloading." -ForegroundColor Green
Write-Host "Press any key to exit..."
$Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown") | Out-Null
