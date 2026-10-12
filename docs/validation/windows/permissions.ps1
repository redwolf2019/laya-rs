# Run from an interactive, non-elevated session of a different Windows user.
param([Parameter(Mandatory=$true)][string]$Assets,
      [Parameter(Mandatory=$true)][string]$PrivateRoot,
      [string]$Root=(Join-Path $env:LOCALAPPDATA 'laya-acceptance'))
$ErrorActionPreference='Stop'
$admin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($admin) { throw 'Use a non-elevated session of a different standard user.' }
$Root=$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Root)
$PrivateRoot=$ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($PrivateRoot)
if ($Root -eq $PrivateRoot) { throw 'Acceptance and private deployments must belong to different users.' }
foreach ($item in @(@{Name='token';Access=[IO.FileAccess]::Read},@{Name='current.json';Access=[IO.FileAccess]::Write})) {
    try {
        $stream=[IO.File]::Open((Join-Path $PrivateRoot $item.Name),[IO.FileMode]::Open,$item.Access)
        $stream.Dispose()
        throw 'Another user accessed private deployment state.'
    } catch {
        if ($_.Exception.GetBaseException() -isnot [UnauthorizedAccessException]) { throw }
    }
}
Write-Output 'PASS: another standard user cannot read token or open deployment state for writing'
$env:PATH="$env:windir\System32;$env:windir;$env:windir\System32\WindowsPowerShell\v1.0"
foreach ($command in @('cargo','rustc','python','node')) {
    if (Get-Command $command -ErrorAction SilentlyContinue) { throw 'Development command is available in runtime PATH.' }
}
$script=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..\scripts\laya.ps1'))
try {
    foreach ($action in @('Install','Start','Update','Stop')) {
        $arguments=@('-NoProfile','-NonInteractive','-ExecutionPolicy','RemoteSigned','-File',$script,$action,'-Root',$Root)
        if ($action -in @('Install','Update')) { $arguments+=@('-ReleaseDirectory',$Assets) }
        if ($action -eq 'Install') { $arguments+=@('-Listen','127.0.0.1:18089') }
        & powershell.exe @arguments
        if ($LASTEXITCODE) { throw "Standard-user $action failed." }
    }
    Write-Output 'PASS: non-administrator install/start/update/real inference/stop with runtime-only PATH'
} finally {
    if (Test-Path (Join-Path $Root 'process.json')) {
        & powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $script Stop -Root $Root
        if ($LASTEXITCODE) { throw 'Standard-user cleanup failed; retain deployment record.' }
    }
}
# Keep the user-owned deployment for inspection; no account or policy changes.
exit 0
