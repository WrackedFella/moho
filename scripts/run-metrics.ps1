# Run rust-code-analysis metrics locally
# This script runs the same metrics analysis as the CI workflow

param(
    [string]$RcaCliPath = ".\_rca\cli\debug\rust-code-analysis-cli.exe",
    [switch]$Clean
)

Write-Host "=== Moho Code Metrics Runner ===" -ForegroundColor Cyan

# Check if CLI exists
if (-not (Test-Path $RcaCliPath)) {
    Write-Host "ERROR: rust-code-analysis-cli not found at: $RcaCliPath" -ForegroundColor Red
    Write-Host "Please build the CLI first from the mozilla/rust-code-analysis repo" -ForegroundColor Yellow
    Write-Host "Or update the -RcaCliPath parameter to point to your CLI executable" -ForegroundColor Yellow
    exit 1
}

# Clean previous results if requested
if ($Clean) {
    Write-Host "Cleaning previous metrics..." -ForegroundColor Yellow
    Remove-Item -Path "metrics-output" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item -Path "metrics-summary" -Recurse -Force -ErrorAction SilentlyContinue
}

# Create output directories
Write-Host "Creating output directories..." -ForegroundColor Gray
New-Item -ItemType Directory -Force -Path "metrics-output", "metrics-summary" | Out-Null

# Run metrics on all source directories
$sourceDirs = @(
    "src",
    "moho_core\src",
    "moho_audio\src",
    "moho_input\src",
    "moho_renderer\src",
    "moho_sim\src",
    "moho_ui\src"
)

Write-Host "`nRunning rust-code-analysis on source directories..." -ForegroundColor Cyan
foreach ($dir in $sourceDirs) {
    if (Test-Path $dir) {
        Write-Host "  Analyzing $dir..." -ForegroundColor Gray
        & $RcaCliPath -m -p $dir -l rust -O json -o metrics-output --pr 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "  Failed to analyze $dir (exit code: $LASTEXITCODE)"
        }
    } else {
        Write-Warning "  Directory not found: $dir"
    }
}

# Count generated files
$fileCount = (Get-ChildItem -Path metrics-output -Recurse -Filter *.json).Count
Write-Host "`nGenerated $fileCount metric files" -ForegroundColor Green

# Run aggregation script
Write-Host "`nAggregating metrics..." -ForegroundColor Cyan
& .\scripts\aggregate-metrics.ps1

Write-Host "`nMetrics analysis complete!" -ForegroundColor Green
Write-Host "Results available in:" -ForegroundColor Cyan
Write-Host "  - metrics-output/     (detailed per-file metrics)" -ForegroundColor Gray
Write-Host "  - metrics-summary/    (aggregated reports)" -ForegroundColor Gray
