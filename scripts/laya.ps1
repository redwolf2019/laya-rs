# Windows PowerShell 5.1. ASCII source; JSON and persistent text use UTF-8.
[CmdletBinding()]
param(
    [Parameter(Mandatory=$true)][ValidateSet('Install','Start','Stop','Update')][string]$Action,
    [string]$Root = (Join-Path $env:LOCALAPPDATA 'laya-server'),
    [string]$Version = 'latest',
    [string]$ReleaseBase = 'https://github.com/redwolf2019/laya-rs/releases',
    [string]$ReleaseDirectory,
    [string]$ModelDirectory,
    [string]$Listen = '0.0.0.0:8080',
    [ValidateRange(1,2147483647)][int]$Threads = 2,
    [ValidateRange(1,2147483647)][int]$InterOpThreads = 1,
    [ValidateRange(1,1024)][int]$MaxConcurrency = 1,
    [ValidateRange(1,86400)][int]$ShutdownGrace = 120,
    [switch]$Recover
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Add-Type -AssemblyName System.IO.Compression.FileSystem
Add-Type -AssemblyName System.Web.Extensions
$Utf8 = New-Object Text.UTF8Encoding($false)
$Sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
$InvocationParameters = $PSBoundParameters
$Root = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Root)
if ($Root.TrimEnd('\') -eq [IO.Path]::GetPathRoot($Root).TrimEnd('\')) { throw 'Use a dedicated deployment directory, not a volume root.' }
$Root = $Root.TrimEnd('\')
$Lock = $null

function Assert-Platform {
    $os = Get-CimInstance Win32_OperatingSystem
    if (-not [Environment]::Is64BitProcess -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64' -or
        [int]$os.BuildNumber -lt 22000) {
        throw 'Windows 11 x64 and 64-bit Windows PowerShell are required.'
    }
    if ($ReleaseBase -notmatch '^https://[^/?#]+/[^?#]*$') { throw 'ReleaseBase must use HTTPS.' }
    foreach ($name in @('Listen','Threads','InterOpThreads','MaxConcurrency','ShutdownGrace')) {
        if ($Action -ne 'Install' -and $InvocationParameters.ContainsKey($name)) { throw 'Change configuration through Install only.' }
    }
}

function Assert-NoLinks([string]$Path) {
    $pathItem = [IO.Path]::GetFullPath($Path)
    while ($pathItem) {
        if (Test-Path -LiteralPath $pathItem) {
            $item = Get-Item -LiteralPath $pathItem -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Reparse points are not allowed in managed paths.' }
        }
        $parent = [IO.Path]::GetDirectoryName($pathItem)
        if ($parent -eq $pathItem) { break }
        $pathItem = $parent
    }
}

function Protect-Directory([string]$Path) {
    $acl = New-Object Security.AccessControl.DirectorySecurity
    $acl.SetOwner($Sid)
    $acl.SetAccessRuleProtection($true, $false)
    foreach ($identity in @($Sid, (New-Object Security.Principal.SecurityIdentifier('S-1-5-18')))) {
        $rule = New-Object Security.AccessControl.FileSystemAccessRule($identity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $Path -AclObject $acl
}

function Open-Root {
    Assert-NoLinks $Root
    if (-not (Test-Path -LiteralPath $Root)) { New-Item -ItemType Directory $Root | Out-Null }
    $marker = Join-Path $Root '.laya-owner'
    if (Test-Path -LiteralPath $marker) {
        if ([IO.File]::ReadAllText($marker) -ne $Sid.Value) { throw 'Deployment belongs to another user.' }
        $acl = Get-Acl -LiteralPath $Root
        if ($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -ne $Sid.Value) { throw 'Unexpected deployment owner.' }
    } elseif (@(Get-ChildItem -LiteralPath $Root -Force).Count) {
        throw 'Refusing to take over a nonempty unmanaged directory.'
    }
    Protect-Directory $Root
    [IO.File]::WriteAllText($marker, $Sid.Value, $Utf8)
    foreach ($name in @('programs','models','staging')) {
        $path = Join-Path $Root $name
        Assert-NoLinks $path
        New-Item -ItemType Directory -Force $path | Out-Null
    }
}

function Read-Json([string]$Name) {
    $path = Join-Path $Root $Name
    Assert-NoLinks $path
    if (Test-Path -LiteralPath $path) { return [IO.File]::ReadAllText($path, $Utf8) | ConvertFrom-Json }
    return $null
}

function Write-Json([string]$Name, $Value) {
    $path = Join-Path $Root $Name
    Assert-NoLinks $path
    Assert-NoLinks "$path.tmp"
    [IO.File]::WriteAllText("$path.tmp", ($Value | ConvertTo-Json -Depth 12), $Utf8)
    if (Test-Path -LiteralPath $path) { [IO.File]::Replace("$path.tmp", $path, [NullString]::Value) }
    else { [IO.File]::Move("$path.tmp", $path) }
}

function Download([string]$Url, [string]$Destination) {
    if ($Url -notmatch '^https://') { throw 'Download requires HTTPS.' }
    $request = [Net.HttpWebRequest]::Create($Url)
    $request.Timeout = 600000
    $request.ReadWriteTimeout = 600000
    $request.AllowAutoRedirect = $false
    $response = $null
    for ($redirect = 0; $redirect -lt 8; $redirect++) {
        $response = $request.GetResponse()
        if ([int]$response.StatusCode -notin @(301,302,303,307,308)) { break }
        $next = New-Object Uri($request.RequestUri, $response.Headers['Location'])
        $response.Dispose()
        if ($next.Scheme -ne 'https') { throw 'Refusing non-HTTPS redirect.' }
        $request = [Net.HttpWebRequest]::Create($next)
        $request.Timeout = 600000; $request.ReadWriteTimeout = 600000; $request.AllowAutoRedirect = $false
        $response = $null
    }
    if ($null -eq $response -or [int]$response.StatusCode -ne 200) { throw 'Download did not return HTTP 200.' }
    try {
        $file = [IO.File]::Create($Destination)
        try { $response.GetResponseStream().CopyTo($file) } finally { $file.Dispose() }
    } finally { $response.Dispose() }
}

function Read-Metadata([string]$Path) {
    $values = @{}
    foreach ($line in [IO.File]::ReadAllLines($Path)) {
        if ($line -notmatch '^([A-Z0-9_a-z]+)=([^\r\n]+)$' -or $values.ContainsKey($Matches[1])) { throw 'Invalid release metadata.' }
        $values[$Matches[1]] = $Matches[2]
    }
    foreach ($key in @('VERSION','MODEL_ID','MODEL_SHA256','MODEL_BYTES','WINDOWS_x86_64_SHA256','WINDOWS_x86_64_BYTES')) {
        if (-not $values.ContainsKey($key)) { throw 'Release has no Windows asset metadata.' }
    }
    if ($values.VERSION -notmatch '^v[0-9]+[A-Za-z0-9.+-]*$' -or $values.MODEL_ID -notmatch '^[0-9a-f]{16}$') { throw 'Invalid release identifiers.' }
    foreach ($key in @('MODEL_SHA256','WINDOWS_x86_64_SHA256')) {
        if ($values[$key] -notmatch '^[0-9a-f]{64}$') { throw 'Invalid release hash.' }
    }
    foreach ($key in @('MODEL_BYTES','WINDOWS_x86_64_BYTES')) {
        if ($values[$key] -notmatch '^[1-9][0-9]{0,10}$' -or [long]$values[$key] -gt 4294967296) { throw 'Invalid release size.' }
    }
    return $values
}

function Fetch-Asset([string]$Name, [string]$Target, [string]$Hash, [long]$Size, [string]$Tag) {
    if ($ReleaseDirectory) { Copy-Item -LiteralPath (Join-Path $ReleaseDirectory $Name) -Destination $Target }
    else { Download "$ReleaseBase/download/$Tag/$Name" $Target }
    if ((Get-Item -LiteralPath $Target).Length -ne $Size -or (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Hash) {
        throw 'Asset size or SHA-256 verification failed; existing deployment is unchanged.'
    }
}

function Archive-Path([string]$Destination, [string]$Name) {
    $normalized = $Name.Replace('\','/').TrimEnd('/')
    if (-not $normalized -or $normalized.StartsWith('/') -or $normalized -match '[:\x00-\x1f]') { throw 'Unsafe archive path.' }
    foreach ($part in $normalized.Split('/')) {
        if (-not $part -or $part -in @('.','..') -or $part -match '[. ]$|[<>"|?*]' -or
            $part -match '^(?i:CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])(?:\.|$)') { throw 'Unsafe archive path.' }
    }
    $full = [IO.Path]::GetFullPath((Join-Path $Destination $normalized))
    if (-not $full.StartsWith($Destination.TrimEnd('\')+'\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Archive escapes destination.' }
    return $full
}

function Expand-Zip([string]$Archive, [string]$Destination) {
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        $seen = @{}
        $expandedBytes = 0L
        if ($zip.Entries.Count -gt 10000) { throw 'Too many ZIP entries.' }
        foreach ($entry in $zip.Entries) {
            $path = Archive-Path $Destination $entry.FullName
            $mode = ($entry.ExternalAttributes -shr 16) -band 61440
            if ($mode -notin @(0,32768,16384) -or ($entry.ExternalAttributes -band 1024) -or $seen.ContainsKey($path)) { throw 'Links or duplicate ZIP entries are forbidden.' }
            $seen[$path] = $true
            $expandedBytes += $entry.Length
            if ($expandedBytes -gt 4294967296) { throw 'ZIP expansion exceeds the package size limit.' }
        }
        foreach ($entry in $zip.Entries) {
            $path = Archive-Path $Destination $entry.FullName
            if ($entry.FullName.EndsWith('/')) { New-Item -ItemType Directory -Force $path | Out-Null }
            else {
                New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($path)) | Out-Null
                [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $path, $false)
            }
        }
    } finally { $zip.Dispose() }
}

function Read-Exact($Stream, [byte[]]$Buffer, [int]$Count) {
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($Buffer,$offset,$Count-$offset)
        if (-not $read) { throw 'Truncated model archive.' }
        $offset += $read
    }
}

function Copy-Bytes($InputStream, $OutputStream, [long]$Count) {
    $buffer = New-Object byte[] 65536
    while ($Count -gt 0) {
        $chunk = [int][Math]::Min($Count,$buffer.Length)
        Read-Exact $InputStream $buffer $chunk
        if ($null -ne $OutputStream) { $OutputStream.Write($buffer,0,$chunk) }
        $Count -= $chunk
    }
}

function Expand-Model([string]$Archive, [string]$Destination) {
    # shortcut: accept only fixed USTAR ordinary entries; add PAX support if the model publisher needs it.
    foreach ($extract in @($false,$true)) {
        $raw = [IO.File]::OpenRead($Archive)
        $gzip = New-Object IO.Compression.GZipStream($raw,[IO.Compression.CompressionMode]::Decompress)
        try { Read-ModelTar $gzip $Destination $extract }
        finally { $gzip.Dispose(); $raw.Dispose() }
    }
}

function Read-ModelTar($Stream, [string]$Destination, [bool]$Extract) {
    $seen = @{}; $total = 0L
    $header = New-Object byte[] 512
    while ($true) {
        Read-Exact $Stream $header 512
        if (-not ($header | Where-Object { $_ -ne 0 })) { break }
        $checksumText = [Text.Encoding]::ASCII.GetString($header,148,8).Trim([char[]]@(0,32))
        $sizeText = [Text.Encoding]::ASCII.GetString($header,124,12).Trim([char[]]@(0,32))
        if ($checksumText -notmatch '^[0-7]+$' -or $sizeText -notmatch '^[0-7]+$') { throw 'Invalid model tar header.' }
        $checksum = 256
        for ($index=0; $index -lt 512; $index++) { if ($index -lt 148 -or $index -ge 156) { $checksum += $header[$index] } }
        if ($checksum -ne [Convert]::ToInt64($checksumText,8)) { throw 'Model tar checksum failed.' }
        if ($header[156] -notin @(0,48,53)) { throw 'Model archive links and special entries are forbidden.' }
        $name = [Text.Encoding]::UTF8.GetString($header,0,100).Split([char]0)[0]
        $prefix = [Text.Encoding]::UTF8.GetString($header,345,155).Split([char]0)[0]
        if ($prefix) { $name = $prefix+'/'+$name }
        $path = Archive-Path $Destination $name
        if ($seen.ContainsKey($path) -or $seen.Count -ge 10000) { throw 'Duplicate or excessive model archive entries.' }
        $seen[$path] = $true
        $size = [Convert]::ToInt64($sizeText,8); $total += $size
        if ($total -gt 4294967296 -or ($header[156] -eq 53 -and $size)) { throw 'Model archive exceeds its size limit.' }
        $output = $null
        if ($Extract) {
            Assert-NoLinks $path
            New-Item -ItemType Directory -Force ([IO.Path]::GetDirectoryName($path)) | Out-Null
            if ($header[156] -eq 53) { New-Item -ItemType Directory -Force $path | Out-Null }
            else { $output = [IO.File]::Open($path,'CreateNew','Write','None') }
        }
        try { Copy-Bytes $Stream $output $size } finally { if ($null -ne $output) { $output.Dispose() } }
        Copy-Bytes $Stream $null ((512-($size % 512)) % 512)
    }
}

function Verify-Model([string]$Directory, [string]$Manifest, [string]$Id) {
    Assert-NoLinks $Directory
    if ((Get-FileHash -LiteralPath $Manifest -Algorithm SHA256).Hash.ToLowerInvariant().Substring(0,16) -ne $Id) { throw 'Wrong fixed model manifest.' }
    # PS 5.1 ConvertFrom-Json rejects the fixed ONNX opset's empty property name.
    $json = New-Object Web.Script.Serialization.JavaScriptSerializer
    $data = $json.DeserializeObject([IO.File]::ReadAllText($Manifest, $Utf8))
    foreach ($entry in $data.files) {
        $path = Archive-Path $Directory $entry.path
        Assert-NoLinks $path
        if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -ne $entry.bytes -or
            (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw 'Fixed model is missing or corrupt.' }
    }
}

function Verify-Program([string]$Directory, [string]$Tag, [string]$Id) {
    Assert-NoLinks $Directory
    if ([IO.File]::ReadAllText((Join-Path $Directory 'VERSION')).Trim() -ne $Tag -or
        [IO.File]::ReadAllText((Join-Path $Directory 'MODEL_ID')).Trim() -ne $Id) { throw 'Wrong program or model identifier.' }
    $entries = [IO.File]::ReadAllText((Join-Path $Directory 'program-files.json'), $Utf8) | ConvertFrom-Json
    foreach ($entry in $entries) {
        $path = Archive-Path $Directory $entry.path
        Assert-NoLinks $path
        if ((Get-Item -LiteralPath $path).Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) {
            throw 'Program files are corrupt.'
        }
    }
    foreach ($name in @('laya-server.exe','onnxruntime.dll','model-manifest.json','smoke.json','VC_MIN')) {
        if (-not ($entries.path -contains $name)) { throw 'Incomplete Windows package.' }
    }
}

function Ensure-Runtime([string]$Directory) {
    $minimum = [Version]([IO.File]::ReadAllText((Join-Path $Directory 'VC_MIN')).Trim())
    $installed = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64' -ErrorAction SilentlyContinue
    if ($installed -and $installed.Installed -eq 1 -and [Version]($installed.Version.TrimStart('v')) -ge $minimum) { return }
    Write-Host "Microsoft VC++ x64 runtime >= $minimum is required. Only its installer requests UAC."
    if ((Read-Host 'Download and install it from Microsoft? [y/N]') -notmatch '^(?i:y|yes)$') { throw 'VC++ installation was declined.' }
    $path = Join-Path $Root 'staging\vc_redist.x64.exe'
    Download 'https://aka.ms/vc14/vc_redist.x64.exe' $path
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'VC++ installer has no valid Microsoft signature.' }
    Write-Json 'vc-installer.json' @{ Version=(Get-Item $path).VersionInfo.FileVersion; SHA256=(Get-FileHash $path).Hash }
    try { $process = Start-Process $path -ArgumentList '/install /passive /norestart' -Verb RunAs -Wait -PassThru }
    catch { throw 'VC++ elevation cancelled or installer could not start.' }
    if ($process.ExitCode -in @(3010,1641)) { throw 'VC++ installation requires a reboot. Restart Windows yourself before retrying.' }
    if ($process.ExitCode -ne 0) { throw 'VC++ installation failed.' }
    $installed = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64'
    if ($installed.Installed -ne 1 -or [Version]($installed.Version.TrimStart('v')) -lt $minimum) { throw 'VC++ runtime remains unavailable.' }
}

function Prepare-Candidate {
    $stage = Join-Path $Root ('staging\'+[Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory $stage | Out-Null
    $metaPath = Join-Path $stage 'release.env'
    $tag = $Version
    if ($ReleaseDirectory) { Copy-Item -LiteralPath (Join-Path $ReleaseDirectory 'release.env') $metaPath }
    else {
        if ($tag -eq 'latest') {
            $latest = Invoke-RestMethod -Uri 'https://api.github.com/repos/redwolf2019/laya-rs/releases/latest' -TimeoutSec 60
            if ($latest.draft -or $latest.prerelease) { throw 'Latest release is not stable.' }
            $tag = $latest.tag_name
        }
        if ($tag -notmatch '^v[0-9]+[A-Za-z0-9.+-]*$') { throw 'Invalid version.' }
        Download "$ReleaseBase/download/$tag/release.env" $metaPath
    }
    $meta = Read-Metadata $metaPath
    if ($tag -ne 'latest' -and $tag -ne $meta.VERSION) { throw 'Release version mismatch.' }
    $tag = $meta.VERSION
    $archive = Join-Path $stage 'program.zip'
    Fetch-Asset 'laya-server-windows-x86_64.zip' $archive $meta.WINDOWS_x86_64_SHA256 ([long]$meta.WINDOWS_x86_64_BYTES) $tag
    $program = Join-Path $stage 'program'
    Expand-Zip $archive $program
    Verify-Program $program $tag $meta.MODEL_ID
    Ensure-Runtime $program
    $model = Prepare-Model $stage $program $meta
    $destination = Join-Path $Root ('programs\'+$tag+'-'+$meta.WINDOWS_x86_64_SHA256.Substring(0,16))
    if (Test-Path -LiteralPath $destination) { Verify-Program $destination $tag $meta.MODEL_ID }
    else { [IO.Directory]::Move($program, $destination) }
    Remove-Item -LiteralPath $stage -Recurse -Force
    return @{ Program=$destination; Model=$model; Version=$tag; ModelId=$meta.MODEL_ID }
}

function Prepare-Model([string]$Stage, [string]$Program, $Metadata) {
    $manifest = Join-Path $Program 'model-manifest.json'
    if ($ModelDirectory) {
        $model = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ModelDirectory)
        Verify-Model $model $manifest $Metadata.MODEL_ID
        return $model
    }
    $committed = Read-Json 'current.json'
    if ($null -ne $committed -and $committed.ModelId -eq $Metadata.MODEL_ID) {
        try { Verify-Model $committed.Model $manifest $Metadata.MODEL_ID; return $committed.Model }
        catch { Write-Host 'Committed model is corrupt; staging a separate replacement.' }
    }
    $model = Join-Path $Root ('models\'+$Metadata.MODEL_ID)
    if (Test-Path -LiteralPath $model) {
        try { Verify-Model $model $manifest $Metadata.MODEL_ID; return $model }
        catch { Write-Host 'Cached model is corrupt; downloading a separate replacement.' }
    }
    $archive = Join-Path $Stage 'model.tar.gz'
    Fetch-Asset ('model-'+$Metadata.MODEL_ID+'.tar.gz') $archive $Metadata.MODEL_SHA256 ([long]$Metadata.MODEL_BYTES) ('model-'+$Metadata.MODEL_ID)
    $unpacked = Join-Path $Stage 'model'
    Expand-Model $archive $unpacked
    Verify-Model $unpacked $manifest $Metadata.MODEL_ID
    if (Test-Path -LiteralPath $model) { $model += '-'+[Guid]::NewGuid().ToString('N') }
    [IO.Directory]::Move($unpacked, $model)
    return $model
}

function Ensure-Token {
    $path = Join-Path $Root 'token'
    Assert-NoLinks $path
    if (-not (Test-Path -LiteralPath $path)) {
        $bytes = New-Object byte[] 32
        $random = [Security.Cryptography.RandomNumberGenerator]::Create()
        try { $random.GetBytes($bytes) } finally { $random.Dispose() }
        [IO.File]::WriteAllText($path, [Convert]::ToBase64String($bytes), $Utf8)
    }
    $acl = New-Object Security.AccessControl.FileSecurity
    $acl.SetOwner($Sid); $acl.SetAccessRuleProtection($true,$false)
    foreach ($identity in @($Sid, (New-Object Security.Principal.SecurityIdentifier('S-1-5-18')))) {
        $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule($identity,'FullControl','Allow')))
    }
    Set-Acl -LiteralPath $path -AclObject $acl
    $token = [IO.File]::ReadAllText($path, $Utf8)
    if ($token -notmatch '^[A-Za-z0-9._~+/-]+=*$') { throw 'Invalid local token file.' }
    return $token
}

function Owned-Process {
    $record = Read-Json 'process.json'
    if ($null -eq $record) { return $null }
    if ($record.Root -ne $Root -or $record.Owner -ne $Sid.Value -or $record.Instance -notmatch '^[0-9a-f]{32}$') { throw 'Process record belongs to another deployment.' }
    $exe = [IO.Path]::GetFullPath($record.Executable)
    if (-not $exe.StartsWith((Join-Path $Root 'programs')+'\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unmanaged executable in process record.' }
    $process = Get-Process -Id $record.ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process) { return $null }
    $cim = Get-CimInstance Win32_Process -Filter "ProcessId=$($record.ProcessId)"
    $owner = Invoke-CimMethod -InputObject $cim -MethodName GetOwnerSid
    if ($owner.ReturnValue -ne 0 -or $owner.Sid -ne $Sid.Value -or $process.Path -ne $exe -or
        $process.StartTime.ToUniversalTime().Ticks.ToString() -ne $record.StartTicks) { throw 'PID ownership cannot be confirmed; refusing to stop or adopt it.' }
    return $process
}

function Stop-Owned([int]$Grace) {
    $process = Owned-Process
    if ($null -eq $process) { return }
    # Cache a query handle before exit; Get-Process otherwise loses the exit code.
    $process.Handle | Out-Null
    $record = Read-Json 'process.json'
    $stop = Join-Path $Root ($record.Instance+'.stop')
    Assert-NoLinks $stop
    [IO.File]::WriteAllBytes($stop, (New-Object byte[] 0))
    if (-not $process.WaitForExit(($Grace+15)*1000)) { throw 'Managed process did not exit; no version switch was made.' }
    if ($process.ExitCode -ne 0) { Write-Warning 'Managed process exited nonzero (check grace-expiry or runtime diagnostics).' }
    Remove-Item -LiteralPath $stop
    Remove-Item -LiteralPath (Join-Path $Root 'process.json')
}

function Quote-Argument([string]$Value) {
    return '"'+[regex]::Replace([regex]::Replace($Value, '(\\*)"', '$1$1\"'), '(\\+)$', '$1$1')+'"'
}

function Start-Owned($Config) {
    if (Owned-Process) { throw 'A managed instance is already running.' }
    Verify-Program $Config.Program $Config.Version $Config.ModelId
    Verify-Model $Config.Model (Join-Path $Config.Program 'model-manifest.json') $Config.ModelId
    Ensure-Runtime $Config.Program
    $instance = [Guid]::NewGuid().ToString('N')
    $exe = Join-Path $Config.Program 'laya-server.exe'
    $args = @('--model',$Config.Model,'--listen',$Config.Listen,'--threads',"$($Config.Threads)",
        '--inter-op-threads',"$($Config.InterOpThreads)",'--max-concurrency',"$($Config.MaxConcurrency)",
        '--shutdown-grace',"$($Config.ShutdownGrace)")
    $oldToken = $env:LAYA_API_TOKEN; $oldStop = $env:LAYA_STOP_FILE
    try {
        $env:LAYA_API_TOKEN = Ensure-Token
        $env:LAYA_STOP_FILE = Join-Path $Root ($instance+'.stop')
        $process = Start-Process -FilePath $exe -ArgumentList (($args | ForEach-Object { Quote-Argument $_ }) -join ' ') -WorkingDirectory $Config.Program -PassThru
    } finally { $env:LAYA_API_TOKEN = $oldToken; $env:LAYA_STOP_FILE = $oldStop }
    try {
        Write-Json 'process.json' @{ ProcessId=$process.Id; StartTicks=$process.StartTime.ToUniversalTime().Ticks.ToString();
            Executable=$exe; Root=$Root; Owner=$Sid.Value; Instance=$instance }
    } catch {
        [IO.File]::WriteAllBytes((Join-Path $Root ($instance+'.stop')), (New-Object byte[] 0))
        if (-not $process.WaitForExit(([int]$Config.ShutdownGrace+15)*1000)) { throw 'Cannot record or reclaim new process; manual recovery is required.' }
        throw 'Cannot record new process; it was reclaimed.'
    }
    return $process
}

function Http-Request([string]$Url, [string]$Method, [string]$Token, [byte[]]$Body) {
    $request = [Net.HttpWebRequest]::Create($Url)
    $request.Proxy = $null; $request.Timeout = 120000; $request.ReadWriteTimeout = 120000
    $request.Method = $Method
    if ($Token) { $request.Headers['Authorization'] = 'Bearer '+$Token }
    if ($Body) {
        $request.ContentType = 'application/json; charset=utf-8'; $request.ContentLength = $Body.Length
        $stream = $request.GetRequestStream()
        try { $stream.Write($Body,0,$Body.Length) } finally { $stream.Dispose() }
    }
    try { $response = $request.GetResponse() }
    catch [Net.WebException] {
        if ($null -eq $_.Exception.Response) { throw 'Local HTTP request failed.' }
        $response = $_.Exception.Response
    }
    try {
        $reader = New-Object IO.StreamReader($response.GetResponseStream(), $Utf8)
        try { $text = $reader.ReadToEnd() } finally { $reader.Dispose() }
        return @{ Status=[int]$response.StatusCode; Text=$text }
    } finally { $response.Dispose() }
}

function Assert-Listener($Process, [int]$Port) {
    if ($Process.HasExited) { throw 'New process exited before validation.' }
    $listeners = @(Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue)
    if (-not $listeners.Count -or @($listeners | Where-Object OwningProcess -ne $Process.Id).Count) {
        throw 'Listener does not belong exclusively to the managed process.'
    }
    $owned = Owned-Process
    if ($null -eq $owned -or $owned.Id -ne $Process.Id) { throw 'Managed process changed during validation.' }
}

function Validate-Running($Config, $Process) {
    if ($Config.Listen -notmatch '^(\[[0-9a-fA-F:]+\]|[0-9.]+):([1-9][0-9]*)$') { throw 'Listen must be a numeric IP address and nonzero port.' }
    $port = [int]$Matches[2]
    $address = $Matches[1]
    $parsedAddress = $null
    if ($port -gt 65535 -or -not [Net.IPAddress]::TryParse($address.Trim('[',']'),[ref]$parsedAddress)) { throw 'Invalid listen endpoint.' }
    if ($address -eq '0.0.0.0') { $address = '127.0.0.1' }
    if ($address -eq '[::]') { $address = '[::1]' }
    $base = 'http://'+$address+':'+$port
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $ready = $false
    while ($clock.Elapsed.TotalSeconds -lt 180) {
        if ($Process.HasExited) { throw 'Candidate failed during startup.' }
        try {
            Assert-Listener $Process $port
            $probe = Http-Request ($base+'/readyz') 'GET' '' $null
            if ($probe.Status -eq 200 -and ($probe.Text | ConvertFrom-Json).status -eq 'ready') { $ready = $true; break }
        } catch { Start-Sleep -Milliseconds 250 }
    }
    if (-not $ready) { throw 'Candidate readiness or listener ownership failed.' }
    $token = Ensure-Token
    if ((Http-Request ($base+'/metrics') 'GET' ('X'+$token) $null).Status -ne 401) { throw 'Candidate authentication rejection failed.' }
    if ((Http-Request ($base+'/metrics') 'GET' $token $null).Status -ne 200) { throw 'Candidate authentication failed.' }
    $raw = [IO.File]::ReadAllBytes((Join-Path $Config.Program 'smoke.json'))
    $result = Http-Request ($base+'/v1/system-one') 'POST' $token $raw
    if ($result.Status -ne 200) { throw 'Candidate real inference failed.' }
    Assert-Smoke ($result.Text | ConvertFrom-Json)
    Assert-Listener $Process $port
}

function Assert-Smoke($Response) {
    if ($Response.model -ne 'rl-agent' -or $Response.usage.input_tokens -le 0 -or $Response.usage.output_tokens -ne 0) { throw 'Invalid inference envelope.' }
    $a = $Response.answers
    if ($a.department.type -ne 'choice' -or $a.priority.type -ne 'score' -or $a.urgent.type -ne 'noul' -or
        $a.department.choice -notin @('billing','technical','sales') -or $a.priority.score -lt 0 -or $a.priority.score -gt 3 -or
        $a.urgent.noul -lt 0 -or $a.urgent.noul -gt 1) { throw 'Invalid three-type inference response.' }
    foreach ($distribution in @($a.department.probabilities,$a.priority.probabilities)) {
        $sum = 0.0
        foreach ($property in $distribution.PSObject.Properties) {
            $v = $property.Value
            if ($v -isnot [ValueType] -or [double]::IsNaN($v) -or $v -lt 0 -or $v -gt 1) { throw 'Invalid probability.' }
            $sum += $v
        }
        if ([Math]::Abs($sum-1) -gt 0.001) { throw 'Invalid probability distribution.' }
    }
}

function Validate-Candidate($Config) {
    $process = Start-Owned $Config
    Validate-Running $Config $process
}

function Restore-Transaction($Transaction) {
    if ($null -ne (Owned-Process)) { Stop-Owned ([int]$Transaction.New.ShutdownGrace) }
    if ($null -ne $Transaction.Old) {
        Validate-Candidate $Transaction.Old
        if (-not $Transaction.WasRunning) { Stop-Owned ([int]$Transaction.Old.ShutdownGrace) }
        Write-Json 'current.json' $Transaction.Old
    } else {
        $current = Join-Path $Root 'current.json'
        if (Test-Path -LiteralPath $current) { Remove-Item -LiteralPath $current }
    }
    Remove-Item -LiteralPath (Join-Path $Root 'transaction.json')
}

function Switch-Candidate($Old, $New, [bool]$WasRunning) {
    $transaction = @{ Old=$Old; New=$New; WasRunning=$WasRunning }
    Write-Json 'transaction.json' $transaction
    try {
        if ($WasRunning) { Stop-Owned ([int]$Old.ShutdownGrace) }
        Validate-Candidate $New
        if (-not $WasRunning) { Stop-Owned ([int]$New.ShutdownGrace) }
        Write-Json 'current.json' $New
    } catch {
        $original = $_.Exception.Message
        Write-Verbose ('Candidate failure: '+$original)
        try { Restore-Transaction $transaction }
        catch {
            Write-Verbose ('Rollback failure: '+$_.Exception.Message)
            throw 'Rollback failed. Backups and transaction.json are retained; use Update -Recover after diagnosing the process.'
        }
        if ($null -eq $Old) { throw ('Initial candidate failed; no committed deployment exists. '+$original) }
        throw ('Candidate failed; previous deployment restored and validated. '+$original)
    }
    if ($null -ne $Old) { Write-Json 'previous.json' $Old }
    Remove-Item -LiteralPath (Join-Path $Root 'transaction.json')
}

function Install-OrUpdate {
    $old = Read-Json 'current.json'
    if ($Action -eq 'Update' -and $null -eq $old) { throw 'No deployment to update; use Install.' }
    $running = $null -ne (Owned-Process)
    if ($running -and $null -eq $old) { throw 'Running process has no committed deployment; recover manually.' }
    $new = Prepare-Candidate
    if ($null -ne $old) {
        foreach ($key in @('Listen','Threads','InterOpThreads','MaxConcurrency','ShutdownGrace')) { $new[$key] = $old.$key }
        if ($Action -eq 'Install') {
            foreach ($key in @('Listen','Threads','InterOpThreads','MaxConcurrency','ShutdownGrace')) {
                if ($InvocationParameters.ContainsKey($key)) { $new[$key] = $InvocationParameters[$key] }
            }
        }
    } else {
        $new.Listen=$Listen; $new.Threads=$Threads; $new.InterOpThreads=$InterOpThreads
        $new.MaxConcurrency=$MaxConcurrency; $new.ShutdownGrace=$ShutdownGrace
    }
    Ensure-Token | Out-Null
    Switch-Candidate $old $new $running
    Write-Host "Prepared and validated $($new.Version). Running state preserved. Token was not printed."
}

try {
    Assert-Platform
    Open-Root
    try { $Lock = [IO.File]::Open((Join-Path $Root 'operation.lock'), 'OpenOrCreate', 'ReadWrite', 'None') }
    catch { throw 'Another deployment operation is running.' }
    $transaction = Read-Json 'transaction.json'
    if ($null -ne $transaction) {
        if (-not $Recover -or $Action -ne 'Update') { throw 'Unfinished transaction detected. Inspect transaction.json, then run Update -Recover.' }
        Restore-Transaction $transaction
        Write-Host 'Previous deployment restored and validated.'
    } elseif ($Recover) { throw 'There is no unfinished transaction to recover.' }
    else {
        switch ($Action) {
            'Install' { Install-OrUpdate }
            'Update' { Install-OrUpdate }
            'Start' {
                $config = Read-Json 'current.json'
                if ($null -eq $config) { throw 'Run Install first.' }
                if (Owned-Process) { throw 'A managed instance is already running.' }
                try { Validate-Candidate $config }
                catch { Stop-Owned ([int]$config.ShutdownGrace); throw }
                Write-Host 'Managed console process started; use Stop from another PowerShell window.'
            }
            'Stop' {
                $config = Read-Json 'current.json'
                if ($null -eq $config) { throw 'Run Install first.' }
                Stop-Owned ([int]$config.ShutdownGrace)
                Write-Host 'Managed process is stopped.'
            }
        }
    }
} catch {
    Write-Host ('laya: '+$_.Exception.Message) -ForegroundColor Red
    Write-Verbose $_.ScriptStackTrace
    exit 1
} finally {
    if ($null -ne $Lock) { $Lock.Dispose() }
}
