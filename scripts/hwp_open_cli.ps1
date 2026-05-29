param(
    [string]$Path,
    [string]$Directory,
    [string]$Pattern = "*.hwp",
    [int]$TimeoutSeconds = 20,
    [switch]$Worker,
    [string]$ResultPath,
    [switch]$NoKillHwpOnTimeout
)

$ErrorActionPreference = "Stop"

function Convert-ToJsonLine($Value) {
    $Value | ConvertTo-Json -Compress -Depth 6
}

function New-Result($Status, $File, $Detail, $ExitCode) {
    [pscustomobject]@{
        status = $Status
        path = $File
        detail = $Detail
        exitCode = $ExitCode
        time = (Get-Date).ToString("o")
    }
}

function Invoke-HwpOpenWorker($File, $OutPath) {
    $hwp = $null
    $result = $null
    try {
        $hwp = New-Object -ComObject HWPFrame.HwpObject
        try {
            $null = $hwp.RegisterModule("FilePathCheckDLL", "FilePathCheckerModule")
        } catch {
        }

        $format = if ([IO.Path]::GetExtension($File).ToLowerInvariant() -eq ".hwpx") { "HWPX" } else { "HWP" }
        $opened = $hwp.Open($File, $format, "forceopen:true")
        if ($opened -eq $true) {
            $result = New-Result "open" $File "Open returned true" 0
        } else {
            $lastError = ""
            try {
                $lastError = [string]$hwp.LastError
            } catch {
            }
            $result = New-Result "fail" $File "Open returned false LastError=$lastError" 1
        }
    } catch {
        $result = New-Result "error" $File $_.Exception.Message 3
    } finally {
        if ($null -ne $hwp) {
            try {
                $hwp.Quit()
            } catch {
            }
            try {
                [Runtime.InteropServices.Marshal]::FinalReleaseComObject($hwp) | Out-Null
            } catch {
            }
        }
        if ($null -ne $result) {
            Convert-ToJsonLine $result | Set-Content -LiteralPath $OutPath -Encoding UTF8
        }
    }
}

if ($Worker) {
    if ([string]::IsNullOrWhiteSpace($Path) -or [string]::IsNullOrWhiteSpace($ResultPath)) {
        throw "-Worker requires -Path and -ResultPath"
    }
    Invoke-HwpOpenWorker -File (Resolve-Path -LiteralPath $Path).Path -OutPath $ResultPath
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Path) -and [string]::IsNullOrWhiteSpace($Directory)) {
    throw "Pass -Path <file> or -Directory <folder>"
}

$scriptPath = $PSCommandPath
if ([string]::IsNullOrWhiteSpace($scriptPath)) {
    throw "Cannot locate current script path"
}

$files = @()
if (-not [string]::IsNullOrWhiteSpace($Path)) {
    $files += (Resolve-Path -LiteralPath $Path).Path
}
if (-not [string]::IsNullOrWhiteSpace($Directory)) {
    $files += Get-ChildItem -LiteralPath $Directory -Filter $Pattern -File | Sort-Object Name | ForEach-Object { $_.FullName }
}

$pwsh32 = Join-Path $env:WINDIR "SysWOW64\WindowsPowerShell\v1.0\powershell.exe"
if (-not (Test-Path -LiteralPath $pwsh32)) {
    $pwsh32 = Join-Path $env:WINDIR "System32\WindowsPowerShell\v1.0\powershell.exe"
}

$userId = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$all = @()

foreach ($file in $files) {
    $guid = [guid]::NewGuid().ToString("N")
    $taskName = "KdsnrHwpOpen-$guid"
    $resultFile = Join-Path $env:TEMP "$taskName.json"
    $args = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", "`"$scriptPath`"",
        "-Worker",
        "-Path", "`"$file`"",
        "-ResultPath", "`"$resultFile`""
    ) -join " "

    $action = New-ScheduledTaskAction -Execute $pwsh32 -Argument $args
    $trigger = New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(5)
    $principal = New-ScheduledTaskPrincipal -UserId $userId -LogonType Interactive -RunLevel Highest

    try {
        Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
        Start-ScheduledTask -TaskName $taskName

        $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
        while ((Get-Date) -lt $deadline) {
            if (Test-Path -LiteralPath $resultFile) {
                break
            }
            Start-Sleep -Milliseconds 250
        }

        if (Test-Path -LiteralPath $resultFile) {
            $json = $null
            for ($i = 0; $i -lt 20; $i++) {
                try {
                    $json = Get-Content -LiteralPath $resultFile -Raw -ErrorAction Stop
                    break
                } catch {
                    Start-Sleep -Milliseconds 100
                }
            }
            if ($null -eq $json) {
                $result = New-Result "launcher_error" $file "Result file remained locked" 4
            } else {
                $result = $json | ConvertFrom-Json
            }
        } else {
            $result = New-Result "timeout" $file "No result after $TimeoutSeconds seconds" 124
            try {
                Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
            } catch {
            }
            if (-not $NoKillHwpOnTimeout) {
                Get-Process -Name Hwp -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
            }
        }
    } catch {
        $result = New-Result "launcher_error" $file $_.Exception.Message 4
    } finally {
        try {
            Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
        } catch {
        }
        try {
            Remove-Item -LiteralPath $resultFile -Force -ErrorAction SilentlyContinue
        } catch {
        }
    }

    $all += $result
    Convert-ToJsonLine $result
}

$failed = @($all | Where-Object { $_.status -ne "open" })
if ($failed.Count -gt 0) {
    exit 1
}
exit 0
