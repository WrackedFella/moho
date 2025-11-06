# Aggregate rust-code-analysis metrics
# This script processes JSON metrics files and generates summary reports
# Compatible with both local Windows PowerShell and GitHub Actions runners

param(
    [string]$MetricsDir = "metrics-output",
    [string]$SummaryDir = "metrics-summary",
    [int]$CyclomaticThreshold = 20,
    [double]$MaintainabilityThreshold = 65.0
)

Write-Host "Aggregating metrics from $MetricsDir to $SummaryDir"
Write-Host "Cyclomatic complexity threshold: $CyclomaticThreshold"
Write-Host "Maintainability index threshold: $MaintainabilityThreshold"

# Ensure summary directory exists
New-Item -ItemType Directory -Force -Path $SummaryDir | Out-Null

# Find all JSON files
$jsonFiles = Get-ChildItem -Path $MetricsDir -Recurse -Filter *.json

Write-Host "Found $($jsonFiles.Count) JSON files"

# Extract high cyclomatic complexity functions
$highCCFunctions = @()
$miPerFile = @()
$fileCount = 0
$skippedCount = 0

foreach ($file in $jsonFiles) {
    try {
        # Read raw JSON as text to avoid PowerShell's case-insensitive key handling
        $jsonText = Get-Content $file.FullName -Raw
        
        # Use .NET JSON serializer which handles case-sensitive keys better
        Add-Type -AssemblyName System.Web.Extensions
        $serializer = New-Object System.Web.Script.Serialization.JavaScriptSerializer
        $serializer.MaxJsonLength = 104857600 # 100MB
        $obj = $serializer.DeserializeObject($jsonText)
        
        $relativePath = $file.FullName.Replace("$PWD\", "").Replace("\", "/")
        
        # Extract path from object
        $filePath = $relativePath
        if ($obj.ContainsKey("path")) {
            $filePath = $obj["path"]
        }
        
        # Extract maintainability index
        $mi = $null
        if ($obj.ContainsKey("metrics")) {
            $metrics = $obj["metrics"]
            if ($metrics -is [System.Collections.IDictionary]) {
                if ($metrics.ContainsKey("mi")) {
                    $miObj = $metrics["mi"]
                    if ($miObj -is [System.Collections.IDictionary] -and $miObj.ContainsKey("mi_visual_studio")) {
                        $mi = [double]$miObj["mi_visual_studio"]
                    }
                }
            }
        }
        
        $miPerFile += [PSCustomObject]@{
            file = $filePath
            mi_visual_studio = $mi
        }
        
        # Extract functions with high cyclomatic complexity
        if ($obj.ContainsKey("spaces")) {
            $spaces = $obj["spaces"]
            if ($spaces -is [System.Collections.IEnumerable]) {
                foreach ($space in $spaces) {
                    if ($space -is [System.Collections.IDictionary] -and $space.ContainsKey("metrics")) {
                        $spaceMetrics = $space["metrics"]
                        if ($spaceMetrics -is [System.Collections.IDictionary]) {
                            # Check for cyclomatic complexity
                            $cc = $null
                            if ($spaceMetrics.ContainsKey("cyclomatic")) {
                                $ccObj = $spaceMetrics["cyclomatic"]
                                if ($ccObj -is [System.Collections.IDictionary] -and $ccObj.ContainsKey("sum")) {
                                    $cc = [double]$ccObj["sum"]
                                } elseif ($ccObj -is [ValueType]) {
                                    $cc = [double]$ccObj
                                }
                            }
                            
                            if ($cc -ne $null -and $cc -gt $CyclomaticThreshold) {
                                $funcName = "unknown"
                                $startLine = $null
                                
                                if ($space.ContainsKey("name")) {
                                    $funcName = $space["name"]
                                }
                                if ($space.ContainsKey("start_line")) {
                                    $startLine = $space["start_line"]
                                }
                                
                                $highCCFunctions += [PSCustomObject]@{
                                    file = $filePath
                                    function = $funcName
                                    cyclomatic = $cc
                                    start_line = $startLine
                                }
                            }
                        }
                    }
                }
            }
        }
        
        $fileCount++
        if ($fileCount % 10 -eq 0) {
            Write-Host "Processed $fileCount files..."
        }
        
    } catch {
        $skippedCount++
        Write-Warning "Skipping $($file.Name): $($_.Exception.Message)"
    }
}

Write-Host "`nProcessed $fileCount files successfully, skipped $skippedCount"

# Write high CC functions report
$highCCPath = Join-Path $SummaryDir "high_cc_functions.json"
$highCCFunctions | ConvertTo-Json -Depth 10 | Out-File -Encoding UTF8 $highCCPath
Write-Host "Found $($highCCFunctions.Count) functions with cyclomatic complexity > $CyclomaticThreshold"
Write-Host "Written to: $highCCPath"

# Write MI per file report
$miPath = Join-Path $SummaryDir "mi_per_file.json"
$miPerFile | ConvertTo-Json -Depth 10 | Out-File -Encoding UTF8 $miPath
Write-Host "Written maintainability index for $($miPerFile.Count) files"
Write-Host "Written to: $miPath"

# Generate summary statistics
Write-Host "`n=== Summary Statistics ==="

if ($highCCFunctions.Count -gt 0) {
    Write-Host "`nTop 10 functions by cyclomatic complexity:"
    $highCCFunctions | 
        Sort-Object -Property cyclomatic -Descending | 
        Select-Object -First 10 | 
        Format-Table -Property @{Label="CC"; Expression={[int]$_.cyclomatic}}, function, file, start_line -AutoSize
}

$lowMI = $miPerFile | Where-Object { $_.mi_visual_studio -ne $null -and $_.mi_visual_studio -lt $MaintainabilityThreshold }
if ($lowMI.Count -gt 0) {
    Write-Host "`nFiles with maintainability index below $MaintainabilityThreshold`:"
    $lowMI | 
        Sort-Object -Property mi_visual_studio | 
        Select-Object -First 10 | 
        Format-Table -Property @{Label="MI"; Expression={[int]$_.mi_visual_studio}}, file -AutoSize
} else {
    Write-Host "`nAll files have maintainability index above $MaintainabilityThreshold (good!)"
}

# Calculate average MI for files that have it
$filesWithMI = $miPerFile | Where-Object { $_.mi_visual_studio -ne $null }
if ($filesWithMI.Count -gt 0) {
    $avgMI = ($filesWithMI | Measure-Object -Property mi_visual_studio -Average).Average
    Write-Host "`nAverage maintainability index: $([int]$avgMI) (across $($filesWithMI.Count) files)"
}

Write-Host "`nMetrics aggregation complete!"
