# Load MSVC + Windows SDK if needed, then start the Lanlink desktop app.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path

function Import-VcVars([string]$bat) {
    cmd /c "`"$bat`" && set" | ForEach-Object {
        if ($_ -match "^([^=]+)=(.*)$") {
            [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2])
        }
    }
}

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vcvars = $null
if (Test-Path $vswhere) {
    $found = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find "VC\Auxiliary\Build\vcvars64.bat" 2>$null
    if ($found) { $vcvars = @($found)[0] }
}
if (-not $vcvars) {
    $vcvars = Get-ChildItem -Path @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio",
        "$env:ProgramFiles\Microsoft Visual Studio"
    ) -Filter vcvars64.bat -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\VC\\Auxiliary\\Build\\vcvars64\.bat$' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}
if ($vcvars -and (Test-Path $vcvars)) {
    Import-VcVars $vcvars
}

if (-not (Get-Command link.exe -ErrorAction SilentlyContinue)) {
    $msvc = Get-ChildItem @(
        "${env:ProgramFiles(x86)}\Microsoft Visual Studio\*\*\VC\Tools\MSVC",
        "$env:ProgramFiles\Microsoft Visual Studio\*\*\VC\Tools\MSVC"
    ) -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        Select-Object -First 1
    $sdkRoot = @(
        "${env:ProgramFiles(x86)}\Windows Kits\10",
        "$env:ProgramFiles\Windows Kits\10"
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1
    $sdkLib = $null
    if ($sdkRoot) {
        $sdkLib = Get-ChildItem "$sdkRoot\Lib" -Directory -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            Select-Object -First 1
    }
    if ($msvc -and $sdkLib) {
        $ver = $sdkLib.Name
        $env:Path = "$($msvc.FullName)\bin\Hostx64\x64;$sdkRoot\bin\$ver\x64;" + $env:Path
        $env:LIB = "$($msvc.FullName)\lib\x64;$sdkRoot\Lib\$ver\um\x64;$sdkRoot\Lib\$ver\ucrt\x64;$env:LIB"
        $env:INCLUDE = "$($msvc.FullName)\include;$sdkRoot\Include\$ver\ucrt;$sdkRoot\Include\$ver\um;$sdkRoot\Include\$ver\shared;$sdkRoot\Include\$ver\winrt;$env:INCLUDE"
    }
}

Set-Location $root
npm run desktop
