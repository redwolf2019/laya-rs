# Native MSVC release builder; run in a developer PowerShell or Windows CI.
param([Parameter(Mandatory=$true)][string]$Version, [string]$Out='dist', [string]$OrtArchive)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($Version -notmatch '^v[0-9]+[A-Za-z0-9.+-]*$') { throw 'Invalid version' }
if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -or -not $env:VCToolsRedistDir) { throw 'Use an x64 MSVC developer environment.' }
$Utf8 = New-Object Text.UTF8Encoding($false)
$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$Out = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Out)
New-Item -ItemType Directory -Force $Out | Out-Null
$work = Join-Path $Out ('windows-build-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
try {
    cargo build --release --locked --target x86_64-pc-windows-msvc
    if ($LASTEXITCODE) { throw 'Release build failed' }
    $archive = Join-Path $work 'ort.zip'
    if ($OrtArchive) { Copy-Item -LiteralPath $OrtArchive $archive }
    else {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -TimeoutSec 600 -Uri 'https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-win-x64-1.28.0.zip' -OutFile $archive
    }
    if ((Get-FileHash $archive).Hash.ToLowerInvariant() -ne 'abef733dacbe2f571547a7150b479b5cb9cc0df22f96c24983a42cadb1b4f8bc') { throw 'ORT archive hash mismatch' }
    Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $work 'ort')
    $ort = Join-Path $work 'ort\onnxruntime-win-x64-1.28.0'
    $pkg = Join-Path $work 'package'
    New-Item -ItemType Directory $pkg | Out-Null
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
    Copy-Item (Join-Path $target 'x86_64-pc-windows-msvc\release\laya-server.exe') $pkg
    Copy-Item (Join-Path $ort 'lib\*.dll') $pkg
    $licenses = Join-Path $pkg 'licenses'
    New-Item -ItemType Directory $licenses | Out-Null
    Copy-Item (Join-Path $ort 'LICENSE'),(Join-Path $ort 'ThirdPartyNotices.txt') $licenses
    Copy-Item (Join-Path $repo 'LICENSE'),(Join-Path $repo 'NOTICE.md'),(Join-Path $repo 'Cargo.lock') $licenses
    Copy-Item -Recurse (Join-Path $repo 'licenses') (Join-Path $licenses 'project')
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    foreach ($crate in Get-ChildItem (Join-Path $cargoHome 'registry\src\*\*') -Directory) {
        $notices = @(Get-ChildItem $crate.FullName -File -Recurse | Where-Object { $_.Name -match '(?i)license|copying|copyright|notice' -or $_.Name -eq 'Cargo.toml' })
        foreach ($file in $notices) {
            $dest = Join-Path $licenses ('crates\'+$crate.Name+'\'+$file.FullName.Substring($crate.FullName.Length+1))
            New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($dest)) | Out-Null
            Copy-Item -LiteralPath $file.FullName $dest
        }
    }
    $sysroot = (& rustc --print sysroot).Trim()
    Copy-Item -Recurse (Join-Path $sysroot 'share\doc\rust\licenses') (Join-Path $licenses 'rust')
    Copy-Item (Join-Path $sysroot 'share\doc\rust\COPYRIGHT*.html') (Join-Path $licenses 'rust')
    Copy-Item (Join-Path $repo 'docs\model-manifest.json') $pkg
    Copy-Item (Join-Path $repo 'tools\release\smoke.json') $pkg
    $modelId = (Get-FileHash (Join-Path $pkg 'model-manifest.json')).Hash.ToLowerInvariant().Substring(0,16)
    [IO.File]::WriteAllText((Join-Path $pkg 'MODEL_ID'), "$modelId`n", $Utf8)
    [IO.File]::WriteAllText((Join-Path $pkg 'VERSION'), "$Version`n", $Utf8)
    $crt = Get-ChildItem (Join-Path $env:VCToolsRedistDir 'x64\*\vcruntime140.dll') | Select-Object -First 1
    if (-not $crt) { throw 'Cannot determine MSVC runtime minimum' }
    $vcMin = [Version]($crt.VersionInfo.FileVersion.Split(' ')[0])
    [IO.File]::WriteAllText((Join-Path $pkg 'VC_MIN'), "$vcMin`n", $Utf8)
    $dependencies = @()
    foreach ($pe in Get-ChildItem $pkg -File | Where-Object Extension -in @('.dll','.exe')) {
        $text = (& dumpbin.exe /DEPENDENTS $pe.FullName) -join "`n"
        if ($LASTEXITCODE) { throw 'PE dependency inspection failed' }
        $dependencies += $pe.Name+"`n"+$text
    }
    [IO.File]::WriteAllText((Join-Path $pkg 'pe-dependencies.txt'), ($dependencies -join "`n"), $Utf8)
    $source = 'uncommitted-source-snapshot'
    if (Test-Path (Join-Path $repo '.git')) {
        $source = (& git -C $repo rev-parse HEAD).Trim()
        if ($LASTEXITCODE -or $source -notmatch '^[0-9a-f]{40}$') { throw 'Cannot record checked-out source commit' }
    }
    $sourceFiles = @(Get-Item (Join-Path $repo 'Cargo.toml'),(Join-Path $repo 'Cargo.lock'),(Join-Path $repo 'rust-toolchain.toml'))
    $sourceFiles += @(Get-ChildItem (Join-Path $repo 'src') -File -Filter '*.rs' -Recurse)
    $sourceEntries = @($sourceFiles | Sort-Object FullName | ForEach-Object {
        @{ path=$_.FullName.Substring($repo.Length+1).Replace('\','/'); sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant() }
    })
    [IO.File]::WriteAllText((Join-Path $pkg 'source-files.json'), ($sourceEntries | ConvertTo-Json -Depth 3), $Utf8)
    $toolchain = @{ Rust=(& rustc -Vv) -join "`n"; MSVC=$env:VCToolsVersion; SDK=$env:WindowsSDKVersion; Source=$source;
        ORT='1.28.0'; VCMinimum="$vcMin"; OS=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber) }
    [IO.File]::WriteAllText((Join-Path $pkg 'build.json'), ($toolchain | ConvertTo-Json -Depth 5), $Utf8)
    $entries = @(Get-ChildItem $pkg -File -Recurse | Sort-Object FullName | ForEach-Object {
        @{ path=$_.FullName.Substring($pkg.Length+1).Replace('\','/'); bytes=$_.Length; sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant() }
    })
    [IO.File]::WriteAllText((Join-Path $pkg 'program-files.json'), ($entries | ConvertTo-Json -Depth 3), $Utf8)
    & (Join-Path $pkg 'laya-server.exe') --help | Out-Null
    if ($LASTEXITCODE) { throw 'Packaged executable failed' }
    $zip = Join-Path $Out 'laya-server-windows-x86_64.zip'
    if (Test-Path $zip) { Remove-Item $zip }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::CreateFromDirectory($pkg,$zip)
    Get-FileHash $zip
} finally { Remove-Item -LiteralPath $work -Recurse -Force }
