# Real model process lifecycle, including CPU work after an HTTP timeout.
param([Parameter(Mandatory=$true)][string]$Program,
      [Parameter(Mandatory=$true)][string]$Model,
      [Parameter(Mandatory=$true)][string]$Work)
$ErrorActionPreference = 'Stop'
$Utf8 = New-Object Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Net.Http
$Work = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Work)
New-Item -ItemType Directory -Force $Work | Out-Null
$oldToken=$env:LAYA_API_TOKEN; $oldStop=$env:LAYA_STOP_FILE
$bytes=New-Object byte[] 32
$rng=[Security.Cryptography.RandomNumberGenerator]::Create()
try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
$token=[Convert]::ToBase64String($bytes)
[Net.ServicePointManager]::DefaultConnectionLimit=8

function Wait-Metric($Client,[string]$Expected) {
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt 15) {
        $text=$Client.GetStringAsync('http://127.0.0.1:18088/metrics').GetAwaiter().GetResult()
        if ($text.Contains($Expected)) { return }
        Start-Sleep -Milliseconds 10
    }
    throw 'Real CPU metric deadline expired.'
}

function Start-Real([string]$Case,[int]$Timeout,[int]$Grace) {
    $env:LAYA_API_TOKEN=$token
    $env:LAYA_STOP_FILE=Join-Path $Work ($Case+'.stop')
    if (Test-Path $env:LAYA_STOP_FILE) { Remove-Item $env:LAYA_STOP_FILE }
    $arguments='--model "'+$Model+'" --listen 127.0.0.1:18088 --threads 2 --max-concurrency 1 --inference-timeout '+$Timeout+' --shutdown-grace '+$Grace
    $process=Start-Process -FilePath (Join-Path $Program 'laya-server.exe') -ArgumentList $arguments -WorkingDirectory $env:TEMP -PassThru -RedirectStandardError (Join-Path $Work ($Case+'.log'))
    $process.Handle | Out-Null
    $clock=[Diagnostics.Stopwatch]::StartNew()
    while ($clock.Elapsed.TotalSeconds -lt 60) {
        if ($process.HasExited) { throw 'Real process failed during startup.' }
        try {
            $ready=Invoke-RestMethod 'http://127.0.0.1:18088/readyz' -TimeoutSec 2
            $listeners=@(Get-NetTCPConnection -State Listen -LocalPort 18088)
            if ($ready.status -eq 'ready' -and $listeners.Count -and -not @($listeners | Where-Object OwningProcess -ne $process.Id).Count) { return $process }
        } catch { Start-Sleep -Milliseconds 100 }
    }
    throw 'Real process readiness failed.'
}

function Stop-Real($Process,[string]$Case,[string]$Signal) {
    if ($Signal -eq 'file') { [IO.File]::WriteAllBytes((Join-Path $Work ($Case+'.stop')),[byte[]]@()) }
    else {
        $helper=Join-Path $PSScriptRoot '..\..\..\tools\release\send-console-event.ps1'
        & powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File $helper -TargetPid $Process.Id -Event $Signal
        if ($LASTEXITCODE) { throw 'Real console event failed.' }
    }
}

function Check-Real([string]$Case,[string]$Signal,[bool]$Timed) {
    $process=$null; $client=$null
    try {
        $process=Start-Real $Case $(if ($Timed) {1} else {120}) $(if ($Timed) {1} else {120})
        $client=New-Object Net.Http.HttpClient
        $client.DefaultRequestHeaders.Authorization=New-Object Net.Http.Headers.AuthenticationHeaderValue('Bearer',$token)
        $client.Timeout=[TimeSpan]::FromSeconds(150)
        $questions=[ordered]@{}
        for ($index=0; $index -lt 16; $index++) { $questions["q$index"]=@{type='noul';instructions='Is a refund required?'} }
        $json=@{state=('Customer requests an urgent refund. '*2000);questions=$questions} | ConvertTo-Json -Depth 8
        $content=New-Object Net.Http.StringContent($json,$Utf8,'application/json')
        $running=$client.PostAsync('http://127.0.0.1:18088/v1/system-one',$content)
        Wait-Metric $client "laya_inference_inflight 1`n"
        if ($Timed) {
            $response=$running.GetAwaiter().GetResult()
            if ([int]$response.StatusCode -ne 504) { throw 'Expected a real HTTP inference timeout.' }
            $response.Dispose()
            Wait-Metric $client "laya_inference_inflight 1`n"
        }
        $queuedContent=New-Object Net.Http.StringContent($json,$Utf8,'application/json')
        $queued=$client.PostAsync('http://127.0.0.1:18088/v1/system-one',$queuedContent)
        Wait-Metric $client "laya_queue_size 1`n"
        Stop-Real $process $Case $Signal
        $response=$queued.GetAwaiter().GetResult()
        if ([int]$response.StatusCode -ne 503) { throw 'Queued work did not become unavailable.' }
        $response.Dispose()
        if (-not $Timed) {
            $response=$running.GetAwaiter().GetResult()
            if ([int]$response.StatusCode -ne 200) { throw 'In-flight real inference did not drain.' }
            $response.Dispose()
        }
        if (-not $process.WaitForExit(135000) -or $process.ExitCode -ne [int]$Timed) { throw 'Wrong real process exit status.' }
        $log=[IO.File]::ReadAllText((Join-Path $Work ($Case+'.log')),$Utf8)
        $event=if ($Timed) {'shutdown_grace_expired'} else {'shutdown_complete'}
        if (-not $log.Contains($event) -or $log.Contains($token)) { throw 'Missing shutdown evidence or token leaked.' }
        $listener=New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,18088)
        try { $listener.Start() } finally { $listener.Stop() }
        Write-Output "PASS: real CPU $Case; timed=$Timed queued=503 exit=$($process.ExitCode); port rebind"
    } finally {
        if ($null -ne $client) { $client.Dispose() }
        if ($null -ne $process -and -not $process.HasExited) {
            Stop-Real $process $Case 'file'
            if (-not $process.WaitForExit(135000)) { throw 'Could not reclaim test-owned process.' }
        }
        $env:LAYA_API_TOKEN=$oldToken; $env:LAYA_STOP_FILE=$oldStop
    }
}

Check-Real 'drain' 'file' $false
Check-Real 'timed-file' 'file' $true
Check-Real 'timed-ctrl-c' '0' $true
Check-Real 'timed-ctrl-break' '1' $true
exit 0
