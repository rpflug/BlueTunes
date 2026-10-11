# Run on Windows with stable Rust (MSVC) and Visual Studio C++ Build Tools.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT') { throw 'Run this builder on Windows.' }
Push-Location $PSScriptRoot
try {
    $target = 'x86_64-pc-windows-msvc'
    cargo build --release --locked --target $target --target-dir target
    if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed.' }
    $metadata = cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $version = ($metadata.packages | Where-Object name -eq 'bluetunes').version
    $name = "BlueTunes-$version-windows-x64"
    $stage = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $stage | Out-Null
    try {
        Copy-Item "target/$target/release/bluetunes.exe" "$stage/BlueTunes.exe"
        Copy-Item 'packaging/windows/README.txt' "$stage/README.txt"
        New-Item -ItemType Directory -Force -Path dist | Out-Null
        $zip = Join-Path $PSScriptRoot "dist/$name.zip"
        Compress-Archive -Path "$stage/*" -DestinationPath $zip -Force
        $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
        [IO.File]::WriteAllText("$zip.sha256", "$hash  $name.zip`n")
        Write-Host "Built $zip"
    } finally { Remove-Item -LiteralPath $stage -Recurse -Force }
} finally { Pop-Location }
