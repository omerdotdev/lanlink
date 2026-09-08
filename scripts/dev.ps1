# Load MSVC + Windows SDK, then start the Lanlink desktop app.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path

$vcvars = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path $vcvars) {
    cmd /c "`"$vcvars`" && set" | ForEach-Object {
        if ($_ -match "^([^=]+)=(.*)$") {
            [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2])
        }
    }
}

$msvc = Get-ChildItem "C:\Program Files (x86)\Microsoft Visual Studio\*\BuildTools\VC\Tools\MSVC" -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
$sdkLib = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\Lib" -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending | Select-Object -First 1
if ($msvc -and $sdkLib) {
    $ver = $sdkLib.Name
    $sdkRoot = "C:\Program Files (x86)\Windows Kits\10"
    $env:Path = "$($msvc.FullName)\bin\Hostx64\x64;$sdkRoot\bin\$ver\x64;" + $env:Path
    $env:LIB = "$($msvc.FullName)\lib\x64;$sdkRoot\Lib\$ver\um\x64;$sdkRoot\Lib\$ver\ucrt\x64;$env:LIB"
    $env:INCLUDE = "$($msvc.FullName)\include;$sdkRoot\Include\$ver\ucrt;$sdkRoot\Include\$ver\um;$sdkRoot\Include\$ver\shared;$sdkRoot\Include\$ver\winrt;$env:INCLUDE"
}

$env:CARGO_TARGET_DIR = "E:\cargo-target\lanlink"
Set-Location $root
npm run desktop
