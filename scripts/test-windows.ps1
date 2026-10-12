# Public script boundaries; no learned weights or inference claims.
$ErrorActionPreference = 'Stop'
$Utf8 = New-Object Text.UTF8Encoding($false)
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Script = Join-Path $PSScriptRoot 'laya.ps1'
$Temp = Join-Path $env:TEMP ('laya-boundaries-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $Temp | Out-Null

function Invoke-Case([string]$Name, [string[]]$Arguments, [string]$Expected) {
    $output = ('N' | & powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $Script @Arguments 2>&1) -join "`n"
    if ($LASTEXITCODE -ne 1 -or $output -notlike "*$Expected*") { throw "${Name}: unexpected result: $output" }
    Write-Host "PASS: $Name"
}

function Metadata([string]$Directory, [string]$Archive) {
    $lines = @('VERSION=v0.1.2','MODEL_ID=0123456789abcdef',('MODEL_SHA256='+('0'*64)),'MODEL_BYTES=1',
        ('WINDOWS_x86_64_SHA256='+(Get-FileHash $Archive).Hash.ToLowerInvariant()),
        ('WINDOWS_x86_64_BYTES='+(Get-Item $Archive).Length))
    [IO.File]::WriteAllLines((Join-Path $Directory 'release.env'), $lines, $Utf8)
}

function Tar-Fixture([string]$Path,[string]$Name,[byte]$Type) {
    $header=New-Object byte[] 512
    [Text.Encoding]::ASCII.GetBytes($Name).CopyTo($header,0)
    [Text.Encoding]::ASCII.GetBytes("00000000001`0").CopyTo($header,124)
    for ($i=148;$i -lt 156;$i++) { $header[$i]=32 }
    $header[156]=$Type
    [Text.Encoding]::ASCII.GetBytes("ustar`0").CopyTo($header,257)
    $sum=($header | Measure-Object -Sum).Sum
    [Text.Encoding]::ASCII.GetBytes(([Convert]::ToString([int]$sum,8).PadLeft(6,'0')+"`0 ")).CopyTo($header,148)
    $file=[IO.File]::Create($Path)
    $gzip=New-Object IO.Compression.GZipStream($file,[IO.Compression.CompressionMode]::Compress)
    try { $gzip.Write($header,0,512); $padding=New-Object byte[] 1536; $gzip.Write($padding,0,1536) }
    finally { $gzip.Dispose(); $file.Dispose() }
}

try {
    $tokens=$null; $errors=$null
    [Management.Automation.Language.Parser]::ParseFile($Script,[ref]$tokens,[ref]$errors) | Out-Null
    if ($errors.Count) { throw 'Script parsing failed.' }
    Invoke-Case 'unmanaged directory' @('Stop','-Root',$PSScriptRoot) 'nonempty unmanaged directory'
    Invoke-Case 'HTTPS boundary' @('Install','-Root',(Join-Path $Temp 'http'),'-ReleaseBase','http://example.com/releases') 'must use HTTPS'
    Invoke-Case 'missing deployment' @('Start','-Root',(Join-Path $Temp 'empty')) 'Run Install first'
    $assets = Join-Path $Temp 'assets'
    New-Item -ItemType Directory $assets | Out-Null
    foreach ($name in @('../escape','/absolute','C:/absolute','ads:stream','trailing.','CON.txt')) {
        $archive = Join-Path $assets 'laya-server-windows-x86_64.zip'
        if (Test-Path $archive) { Remove-Item $archive }
        $zip = [IO.Compression.ZipFile]::Open($archive,'Create')
        try { $zip.CreateEntry($name) | Out-Null } finally { $zip.Dispose() }
        Metadata $assets $archive
        $root = Join-Path $Temp ([Guid]::NewGuid().ToString('N'))
        Invoke-Case "archive $name" @('Install','-Root',$root,'-ReleaseDirectory',$assets) 'Unsafe archive path'
    }
    $archive = Join-Path $assets 'laya-server-windows-x86_64.zip'
    Remove-Item $archive
    $zip = [IO.Compression.ZipFile]::Open($archive,'Create')
    try {
        $entry = $zip.CreateEntry('link')
        $entry.ExternalAttributes = -1577058304 # Unix symlink mode 0120000 in the upper word.
    } finally { $zip.Dispose() }
    Metadata $assets $archive
    Invoke-Case 'ZIP symlink' @('Install','-Root',(Join-Path $Temp 'link'),'-ReleaseDirectory',$assets) 'Links or duplicate ZIP'
    [IO.File]::AppendAllText((Join-Path $assets 'release.env'), "VERSION=v0.1.2`n", $Utf8)
    Invoke-Case 'duplicate metadata' @('Install','-Root',(Join-Path $Temp 'duplicate'),'-ReleaseDirectory',$assets) 'Invalid release metadata'
    Metadata $assets $archive
    [IO.File]::AppendAllText($archive,'corruption',$Utf8)
    Invoke-Case 'asset corruption' @('Install','-Root',(Join-Path $Temp 'corrupt'),'-ReleaseDirectory',$assets) 'verification failed'
    $fake = Join-Path $Temp 'fake-program'
    New-Item -ItemType Directory $fake | Out-Null
    Copy-Item (Join-Path $PSScriptRoot '..\docs\model-manifest.json') $fake
    $modelId = (Get-FileHash (Join-Path $fake 'model-manifest.json')).Hash.ToLowerInvariant().Substring(0,16)
    foreach ($pair in @(@('VERSION','v0.1.2'),@('MODEL_ID',$modelId),@('VC_MIN','0.0'),
        @('laya-server.exe','not executable'),@('onnxruntime.dll','not a DLL'),@('smoke.json','{}'))) {
        [IO.File]::WriteAllText((Join-Path $fake $pair[0]),$pair[1],$Utf8)
    }
    $files = @(Get-ChildItem $fake -File | ForEach-Object {
        @{path=$_.Name; bytes=$_.Length; sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant()}
    })
    [IO.File]::WriteAllText((Join-Path $fake 'program-files.json'),($files | ConvertTo-Json),$Utf8)
    Remove-Item $archive
    [IO.Compression.ZipFile]::CreateFromDirectory($fake,$archive)
    Metadata $assets $archive
    $metaPath = Join-Path $assets 'release.env'
    [IO.File]::WriteAllText($metaPath,[IO.File]::ReadAllText($metaPath).Replace('0123456789abcdef',$modelId),$Utf8)
    $emptyModel = Join-Path $Temp 'empty-model'
    New-Item -ItemType Directory $emptyModel | Out-Null
    Invoke-Case 'PS 5.1 fixed manifest (empty opset key)' @('Install','-Root',(Join-Path $Temp 'manifest'),
        '-ReleaseDirectory',$assets,'-ModelDirectory',$emptyModel) 'Fixed model is missing or corrupt'
    foreach ($case in @(@('../escape',48,'Unsafe archive path'),@('link',50,'special entries'),@('pax',120,'special entries'))) {
        $tar=Join-Path $assets ('model-'+$modelId+'.tar.gz')
        Tar-Fixture $tar $case[0] ([byte]$case[1])
        $meta=[IO.File]::ReadAllText($metaPath)
        $meta=[regex]::Replace($meta,'(?m)^MODEL_SHA256=.*$',('MODEL_SHA256='+(Get-FileHash $tar).Hash.ToLowerInvariant()))
        $meta=[regex]::Replace($meta,'(?m)^MODEL_BYTES=.*$',('MODEL_BYTES='+(Get-Item $tar).Length))
        [IO.File]::WriteAllText($metaPath,$meta,$Utf8)
        Invoke-Case ('tar '+$case[0]) @('Install','-Root',(Join-Path $Temp ([Guid]::NewGuid().ToString('N'))),
            '-ReleaseDirectory',$assets) $case[2]
    }
    [IO.File]::WriteAllText((Join-Path $fake 'VC_MIN'),'99.0',$Utf8)
    $files=@(Get-ChildItem $fake -File | Where-Object Name -ne 'program-files.json' | ForEach-Object {
        @{path=$_.Name; bytes=$_.Length; sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant()}
    })
    [IO.File]::WriteAllText((Join-Path $fake 'program-files.json'),($files | ConvertTo-Json),$Utf8)
    Remove-Item $archive
    [IO.Compression.ZipFile]::CreateFromDirectory($fake,$archive)
    Metadata $assets $archive
    [IO.File]::WriteAllText($metaPath,[IO.File]::ReadAllText($metaPath).Replace('0123456789abcdef',$modelId),$Utf8)
    Invoke-Case 'VC++ insufficient and user refuses' @('Install','-Root',(Join-Path $Temp 'vc-declined'),
        '-ReleaseDirectory',$assets) 'installation was declined'
    $lockRoot = Join-Path $Temp 'empty'
    $lock = [IO.File]::Open((Join-Path $lockRoot 'operation.lock'),'Open','ReadWrite','None')
    try { Invoke-Case 'operation lock' @('Stop','-Root',$lockRoot) 'Another deployment operation' }
    finally { $lock.Dispose() }
    [IO.File]::WriteAllText((Join-Path $lockRoot 'transaction.json'),'{"Old":null,"New":{},"WasRunning":false}',$Utf8)
    Invoke-Case 'interrupted transaction' @('Start','-Root',$lockRoot) 'Unfinished transaction'
    $acl = Get-Acl -LiteralPath $lockRoot
    $allowed = @([Security.Principal.WindowsIdentity]::GetCurrent().User.Value,'S-1-5-18')
    foreach ($rule in $acl.Access) {
        if ($rule.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin $allowed) { throw 'Unexpected deployment ACL.' }
    }
    Write-Host 'PASS: private deployment ACL'
} finally { Remove-Item -LiteralPath $Temp -Recurse -Force }
exit 0
