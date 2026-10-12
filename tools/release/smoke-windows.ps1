# Real process installation/update/rollback gate; requires the fixed model archive.
param([Parameter(Mandatory=$true)][string]$ReleaseDirectory,
      [string]$Root=(Join-Path $env:TEMP ('laya smoke '+[Guid]::NewGuid().ToString('N'))))
$ErrorActionPreference = 'Stop'
$Utf8 = New-Object Text.UTF8Encoding($false)
$Script = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\scripts\laya.ps1'))
$Root = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Root)
$ReleaseDirectory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ReleaseDirectory)
$bad = Join-Path $env:TEMP ('laya-bad-'+[Guid]::NewGuid().ToString('N'))

function Run([string]$Action, [string]$Assets=$ReleaseDirectory, [string[]]$Extra=@(), [bool]$Fail=$false) {
    $args = @('-NoProfile','-ExecutionPolicy','RemoteSigned','-File',$Script,$Action,'-Root',$Root)
    if ($Action -in @('Install','Update')) { $args += @('-ReleaseDirectory',$Assets) }
    $output = (& powershell.exe @args @Extra 2>&1) -join "`n"
    if ($Fail) {
        if ($LASTEXITCODE -eq 0 -or $output -notlike '*previous deployment restored and validated*') { throw "Expected real rollback: $output" }
    } elseif ($LASTEXITCODE) { throw "${Action}: $output" }
    Write-Output "PASS: $Action (expected failure=$Fail)"
}

function Current { return [IO.File]::ReadAllText((Join-Path $Root 'current.json'),$Utf8) | ConvertFrom-Json }
function Record { return [IO.File]::ReadAllText((Join-Path $Root 'process.json'),$Utf8) | ConvertFrom-Json }

function Test-Candidate([string]$Fault) {
    New-Item -ItemType Directory -Force $bad | Out-Null
    $unpack = Join-Path $bad 'unpacked'
    if (Test-Path $unpack) { Remove-Item $unpack -Recurse -Force }
    Expand-Archive (Join-Path $ReleaseDirectory 'laya-server-windows-x86_64.zip') $unpack
    $unpack = (Get-Item -LiteralPath $unpack).FullName
    [IO.File]::WriteAllText((Join-Path $unpack 'VERSION'),'v0.1.2-fault-'+$Fault,$Utf8)
    if ($Fault -eq 'startup') { [IO.File]::WriteAllBytes((Join-Path $unpack 'onnxruntime.dll'),[byte[]]@(0)) }
    elseif ($Fault -eq 'api') { [IO.File]::WriteAllText((Join-Path $unpack 'smoke.json'),'{}',$Utf8) }
    $entries = @(Get-ChildItem $unpack -Recurse -File | Where-Object Name -ne 'program-files.json' | ForEach-Object {
        @{ path=$_.FullName.Substring($unpack.Length+1).Replace('\','/'); bytes=$_.Length; sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant() }
    })
    [IO.File]::WriteAllText((Join-Path $unpack 'program-files.json'),($entries | ConvertTo-Json -Depth 3),$Utf8)
    $zip = Join-Path $bad 'laya-server-windows-x86_64.zip'
    if (Test-Path $zip) { Remove-Item $zip }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($unpack,$zip)
    $lines = [IO.File]::ReadAllLines((Join-Path $ReleaseDirectory 'release.env')) | ForEach-Object {
        if ($_ -like 'VERSION=*') { 'VERSION=v0.1.2-fault-'+$Fault }
        elseif ($_ -like 'WINDOWS_x86_64_SHA256=*') { 'WINDOWS_x86_64_SHA256='+(Get-FileHash $zip).Hash.ToLowerInvariant() }
        elseif ($_ -like 'WINDOWS_x86_64_BYTES=*') { 'WINDOWS_x86_64_BYTES='+(Get-Item $zip).Length }
        else { $_ }
    }
    [IO.File]::WriteAllLines((Join-Path $bad 'release.env'),$lines,$Utf8)
    Write-Output "Prepared $Fault candidate; SHA256=$((Get-FileHash $zip).Hash)"
}

try {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    Run 'Install' $ReleaseDirectory @('-Listen','127.0.0.1:18087')
    Write-Output "install_ms=$($clock.ElapsedMilliseconds)"
    if (Test-Path (Join-Path $Root 'process.json')) { throw 'Initial stopped state was not preserved.' }
    $old = Current
    $keyHash = (Get-FileHash (Join-Path $Root 'token')).Hash
    $modelStamp = (Get-Item (Join-Path $old.Model 'laya.onnx.data')).LastWriteTimeUtc.Ticks
    $clock.Restart()
    Run 'Start'
    Write-Output "cold_start_and_three_type_validation_ms=$($clock.ElapsedMilliseconds)"
    if ($clock.ElapsedMilliseconds -gt 200000) { throw 'Start did not return after bounded validation.' }
    $record = Record
    $before = $record.ProcessId
    $output = (& powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $Script Start -Root $Root 2>&1) -join "`n"
    if ($LASTEXITCODE -eq 0 -or (Record).ProcessId -ne $before -or -not (Get-Process -Id $before -ErrorAction SilentlyContinue)) { throw 'Repeated Start stopped or replaced the old process.' }
    Write-Output 'PASS: repeated Start rejects conflict and preserves old process'
    Get-Process -Id $record.ProcessId | Select-Object WorkingSet64,PeakWorkingSet64,TotalProcessorTime | ConvertTo-Json -Compress | Write-Output
    # A forged record must never adopt or stop this unrelated PowerShell process.
    $savedRecord = [IO.File]::ReadAllText((Join-Path $Root 'process.json'),$Utf8)
    $record.ProcessId = $PID
    [IO.File]::WriteAllText((Join-Path $Root 'process.json'),($record | ConvertTo-Json),$Utf8)
    $output = (& powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $Script Stop -Root $Root 2>&1) -join "`n"
    if ($LASTEXITCODE -eq 0 -or $output -notlike '*ownership cannot be confirmed*') { throw 'Forged process record was accepted.' }
    [IO.File]::WriteAllText((Join-Path $Root 'process.json'),$savedRecord,$Utf8)
    Write-Output 'PASS: wrong PID ownership rejected'
    # Corrupt downloads leave the existing process alive and do not switch config.
    Test-Candidate 'api'
    [IO.File]::AppendAllText((Join-Path $bad 'laya-server-windows-x86_64.zip'),'corrupt',$Utf8)
    $before = (Record).ProcessId
    $output = (& powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $Script Update -Root $Root -ReleaseDirectory $bad 2>&1) -join "`n"
    if ($LASTEXITCODE -eq 0 -or (Record).ProcessId -ne $before -or -not (Get-Process -Id $before -ErrorAction SilentlyContinue)) { throw 'Download failure changed the running process.' }
    Write-Output 'PASS: asset verification failure preserves old process'
    # Explicit external model placement survives an update without a model archive.
    Run 'Stop'
    $external = $Root+'-model'
    if (-not (Test-Path $external)) { Move-Item -LiteralPath $old.Model -Destination $external }
    Run 'Install' $ReleaseDirectory @('-ModelDirectory',$external)
    Run 'Start'
    Test-Candidate 'version'
    $versionFile=Join-Path $bad 'unpacked\VERSION'
    $newVersion=[IO.File]::ReadAllText($versionFile,$Utf8)
    Run 'Update' $bad
    $previous=[IO.File]::ReadAllText((Join-Path $Root 'previous.json'),$Utf8) | ConvertFrom-Json
    if ((Current).Version -ne $newVersion -or $previous.Version -ne $old.Version -or (Current).Model -ne $external) { throw 'A-to-B pointers or external model reuse failed.' }
    Write-Output 'PASS: A-to-B version switch and external model reuse without model archive'
    Run 'Update'
    $old=Current
    Run 'Update'
    foreach ($fault in @('startup','api')) {
        Test-Candidate $fault
        Run 'Update' $bad @() $true
        if ((Current).Program -ne $old.Program -or (Get-FileHash (Join-Path $Root 'token')).Hash -ne $keyHash) { throw 'Rollback changed version or token.' }
    }
    if ((Get-Item (Join-Path $old.Model 'laya.onnx.data')).LastWriteTimeUtc.Ticks -ne $modelStamp) { throw 'Valid model cache was replaced.' }
    # Explicit crash-recovery journal, then a real old-process stop/restart/API validation.
    $transaction = @{ Old=(Current); New=(Current); WasRunning=$true }
    [IO.File]::WriteAllText((Join-Path $Root 'transaction.json'),($transaction | ConvertTo-Json -Depth 8),$Utf8)
    Run 'Update' $ReleaseDirectory @('-Recover')
    Run 'Stop'
    Run 'Update'
    if (Test-Path (Join-Path $Root 'process.json')) { throw 'Stopped state changed after update.' }
    if ((Get-FileHash (Join-Path $Root 'token')).Hash -ne $keyHash) { throw 'Token changed.' }
    Write-Output 'PASS: real install/start/stop/update, startup/API rollback, recovery, stopped-state and model/token reuse'
} finally {
    if (Test-Path (Join-Path $Root 'process.json')) {
        & powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $Script Stop -Root $Root
        if ($LASTEXITCODE) { Write-Warning 'Cleanup could not stop managed process; retain its record for diagnosis.' }
    }
    if (Test-Path $bad) { Remove-Item $bad -Recurse -Force }
}
exit 0
