# Builds the xTiger desktop app for release: our own setup program and a portable zip.
#
#   powershell -ExecutionPolicy Bypass -File xtiger-app/build-release.ps1
#
# Needs Rust and the Tauri CLI (`cargo install tauri-cli --version "^2"`). The results land in
# xtiger-app/src-tauri/target/release/bundle/.

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem

$app = $PSScriptRoot
$repo = Split-Path $app -Parent
$tauri = Join-Path $app 'src-tauri'
$setup = Join-Path $app 'setup'
$release = Join-Path $tauri 'target/release'
$bundle = Join-Path $release 'bundle'

function Invoke-Checked([string] $what, [scriptblock] $block) {
    Write-Host "==> $what" -ForegroundColor Cyan
    & $block
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit code $LASTEXITCODE)" }
}

# Zips the contents of a folder. Unlike Compress-Archive in Windows PowerShell, this writes
# forward slashes in the entry names, which every unzip tool understands.
function New-Zip([string] $from, [string] $zip) {
    New-Item -ItemType Directory -Force (Split-Path $zip -Parent) | Out-Null
    Remove-Item $zip -ErrorAction SilentlyContinue
    [System.IO.Compression.ZipFile]::CreateFromDirectory($from, $zip, 'Optimal', $false)
}

$version = (Get-Content "$tauri/tauri.conf.json" -Raw | ConvertFrom-Json).version
$setupVersion = (Get-Content "$setup/tauri.conf.json" -Raw | ConvertFrom-Json).version
if ($setupVersion -ne $version) { throw "The app is version $version but the setup is $setupVersion" }

Remove-Item -Recurse -Force $bundle -ErrorAction SilentlyContinue

# 1. The validator and the MCP server for AI assistants. Tauri ships them next to the app as
# sidecars, named after the target triple.
Invoke-Checked 'Building the validator and the MCP server' {
    cargo build --release -p ck3-tiger -p xtiger-mcp --manifest-path "$repo/Cargo.toml"
}
$triple = (rustc -vV | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
$binaries = Join-Path $tauri 'binaries'
New-Item -ItemType Directory -Force $binaries | Out-Null
foreach ($name in 'ck3-tiger', 'xtiger-mcp') {
    Copy-Item "$repo/target/release/$name.exe" "$binaries/$name-$triple.exe" -Force
}

# 2. The app.
Push-Location $tauri
try {
    Invoke-Checked 'Building the app' { cargo tauri build }
} finally {
    Pop-Location
}

# 3. What gets installed: the three programs and the licenses.
$staging = Join-Path $release 'staging/xTiger'
Remove-Item -Recurse -Force (Split-Path $staging -Parent) -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$staging/licenses" | Out-Null
Copy-Item "$release/xTiger.exe", "$release/ck3-tiger.exe", "$release/xtiger-mcp.exe" $staging
Copy-Item "$repo/LICENSE" "$staging/licenses/LICENSE.txt"
Copy-Item "$app/ui/fonts/OFL-*.txt" "$staging/licenses"

# 4. The setup, with those files packed inside it.
$payload = Join-Path $release 'staging/payload.zip'
New-Zip $staging $payload
$env:XTIGER_SETUP_PAYLOAD = $payload
$env:CARGO_TARGET_DIR = Join-Path $tauri 'target'
Push-Location $setup
try {
    Invoke-Checked 'Building the setup' { cargo tauri build }
} finally {
    Pop-Location
    Remove-Item Env:XTIGER_SETUP_PAYLOAD, Env:CARGO_TARGET_DIR
}
$installer = Join-Path $bundle "xTiger_${version}_x64-setup.exe"
New-Item -ItemType Directory -Force $bundle | Out-Null
Copy-Item "$release/xtiger-setup.exe" $installer -Force

# 5. The portable zip: the same files, plus portable.txt so that the app keeps its settings in
# a data folder next to it.
Set-Content "$staging/portable.txt" -Encoding utf8 -Value @'
This copy of xTiger is portable: it keeps its settings and past runs in the "data" folder
next to it, so you can carry the whole folder around.

Delete this file to keep them in your user profile instead, like the installed app does.
'@
Remove-Item $payload
$portable = Join-Path $bundle "xTiger_${version}_x64_portable.zip"
New-Zip (Split-Path $staging -Parent) $portable

Write-Host ''
Write-Host 'Done:' -ForegroundColor Green
Get-ChildItem $installer, $portable | ForEach-Object { Write-Host "  $($_.FullName)" }
