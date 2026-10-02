param(
    [string]$Executable = (Join-Path $PSScriptRoot "..\target\release\fxdk-agent-desktop.exe"),
    [switch]$SkipSmoke
)

$ErrorActionPreference = "Stop"

function Assert-True {
    param(
        [bool]$Condition,
        [string]$Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Get-DescendantProcesses {
    param([int]$RootProcessId)

    $all = @(Get-CimInstance Win32_Process)
    $ids = New-Object "System.Collections.Generic.HashSet[int]"
    [void]$ids.Add($RootProcessId)

    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($process in $all) {
            if (
                $ids.Contains([int]$process.ParentProcessId) -and
                -not $ids.Contains([int]$process.ProcessId)
            ) {
                [void]$ids.Add([int]$process.ProcessId)
                $changed = $true
            }
        }
    }

    return @(
        $all |
            Where-Object { $ids.Contains([int]$_.ProcessId) } |
            Select-Object ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine
    )
}

function Wait-HttpOk {
    param(
        [string]$Uri,
        [int]$TimeoutSeconds = 30
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Uri $Uri -TimeoutSec 2
            if ($response.StatusCode -eq 200) {
                return
            }
        } catch {
            Start-Sleep -Milliseconds 250
        }
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "Timed out waiting for HTTP 200 from $Uri"
}

$resolvedExecutable = (Resolve-Path $Executable).Path
$item = Get-Item $resolvedExecutable
$version = $item.VersionInfo

Assert-True ($version.ProductName -eq "FXDK Agent") "Unexpected ProductName: $($version.ProductName)"
Assert-True ($version.FileDescription -eq "FXDK Agent") "Unexpected FileDescription: $($version.FileDescription)"
Assert-True (-not [string]::IsNullOrWhiteSpace($version.FileVersion)) "FileVersion is missing"
Assert-True (-not [string]::IsNullOrWhiteSpace($version.ProductVersion)) "ProductVersion is missing"

$hash = (Get-FileHash $resolvedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
$hashPath = "$resolvedExecutable.sha256"
"$hash  $($item.Name)" | Set-Content -Path $hashPath -Encoding ascii

$dependencies = @()
$dumpbin = Get-Command dumpbin.exe -ErrorAction SilentlyContinue
if ($dumpbin) {
    $output = & $dumpbin.Source /DEPENDENTS $resolvedExecutable
    if ($LASTEXITCODE -ne 0) {
        throw "dumpbin failed with exit code $LASTEXITCODE"
    }

    $dependencies = @(
        $output |
            ForEach-Object {
                if ($_ -match "^\s+([A-Za-z0-9._-]+\.dll)\s*$") {
                    $Matches[1]
                }
            } |
            Sort-Object -Unique
    )

    $forbiddenImports = @(
        $dependencies |
            Where-Object { $_ -match "(?i)(^|[-_.])(node|bun|python)([-_.]|$)" }
    )
    Assert-True ($forbiddenImports.Count -eq 0) "Forbidden runtime import(s): $($forbiddenImports -join ', ')"
}

$processTree = @()
$forbiddenProcesses = @()
$smokePerformed = -not $SkipSmoke

if (-not $SkipSmoke) {
    foreach ($port in 35418, 35419) {
        $listener = Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue
        Assert-True (@($listener).Count -eq 0) "Port $port is already in use before standalone smoke"
    }

    $process = $null
    $descendantIds = @()

    try {
        $process = Start-Process -FilePath $resolvedExecutable -WorkingDirectory $item.DirectoryName -PassThru

        Wait-HttpOk "http://127.0.0.1:35418/v1/health"
        Wait-HttpOk "http://127.0.0.1:35419/__fxdk-agent/health"

        $processTree = @(Get-DescendantProcesses -RootProcessId $process.Id)
        $descendantIds = @($processTree | ForEach-Object { [int]$_.ProcessId })

        $forbiddenProcesses = @(
            $processTree |
                Where-Object {
                    $_.Name -match "(?i)^(node|bun|python|python3|py)(\.exe)?$"
                }
        )

        Assert-True ($forbiddenProcesses.Count -eq 0) "Standalone spawned forbidden runtime process(es): $($forbiddenProcesses.Name -join ', ')"
    } finally {
        if ($process -and -not $process.HasExited) {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(5000)) {
                Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
            }
        }

        Start-Sleep -Milliseconds 750

        foreach ($childProcessId in $descendantIds) {
            $remaining = Get-Process -Id $childProcessId -ErrorAction SilentlyContinue
            if ($remaining -and $childProcessId -ne $process.Id) {
                Stop-Process -Id $childProcessId -Force -ErrorAction SilentlyContinue
            }
        }
    }

    Start-Sleep -Milliseconds 500
    foreach ($port in 35418, 35419) {
        $listener = Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue
        Assert-True (@($listener).Count -eq 0) "Port $port is still listening after standalone shutdown"
    }
}

$result = [pscustomobject]@{
    executable = $resolvedExecutable
    size = $item.Length
    sha256 = $hash
    sha256File = $hashPath
    fileVersion = $version.FileVersion
    productVersion = $version.ProductVersion
    productName = $version.ProductName
    companyName = $version.CompanyName
    dependencies = $dependencies
    smokePerformed = $smokePerformed
    processTree = @($processTree | Select-Object ProcessId, ParentProcessId, Name, ExecutablePath)
    forbiddenProcesses = @($forbiddenProcesses | Select-Object ProcessId, Name)
}

$result | ConvertTo-Json -Depth 5
