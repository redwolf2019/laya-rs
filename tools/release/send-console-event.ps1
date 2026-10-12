# Test helper: the target must own a dedicated console, never the user's console.
param([Parameter(Mandatory=$true)][int]$TargetPid, [ValidateSet(0,1)][int]$Event)
$ErrorActionPreference = 'Stop'
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ConsoleEvent {
    public delegate bool Handler(uint type);
    public static Handler Ignore = type => true;
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool FreeConsole();
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool AttachConsole(uint pid);
    [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GenerateConsoleCtrlEvent(uint type, uint group);
    [DllImport("kernel32.dll")] public static extern bool SetConsoleCtrlHandler(Handler handler, bool add);
}
'@
[ConsoleEvent]::FreeConsole() | Out-Null
if (-not [ConsoleEvent]::AttachConsole($TargetPid)) { throw 'Cannot attach test console' }
if (-not [ConsoleEvent]::SetConsoleCtrlHandler([ConsoleEvent]::Ignore, $true)) { throw 'Cannot protect helper' }
if (-not [ConsoleEvent]::GenerateConsoleCtrlEvent($Event, 0)) { throw 'Cannot send console event' }
Start-Sleep -Milliseconds 200
[ConsoleEvent]::FreeConsole() | Out-Null
