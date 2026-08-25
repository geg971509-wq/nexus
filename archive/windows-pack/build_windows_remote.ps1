# Run on Windows host after sources + NexusCore.exe are present under $NexusRoot.
# Builds Tauri shell (NSIS) using prebuilt Core.
param(
  [string]$NexusRoot = "$env:USERPROFILE\NexusBuild"
)

$ErrorActionPreference = "Stop"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

function Ensure-Rust {
  if (Get-Command cargo -ErrorAction SilentlyContinue) { return }
  if (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe") {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    return
  }
  throw "cargo missing; run install_rust_windows.ps1 first"
}

if (-not (Test-Path $NexusRoot)) { throw "NexusRoot missing: $NexusRoot" }
Ensure-Rust
Write-Host ("cargo: " + (cargo -V))

# macOS AppleDouble / Finder junk breaks tauri-build permission scan (non-UTF-8).
Get-ChildItem -Path $NexusRoot -Recurse -Force -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -like '._*' -or $_.Name -eq '.DS_Store' } |
  Remove-Item -Force -Recurse -ErrorAction SilentlyContinue

$bin = Join-Path $NexusRoot "bin"
$binaries = Join-Path $NexusRoot "app\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $bin, $binaries | Out-Null

$coreCandidates = @(
  (Join-Path $bin "NexusCore.exe"),
  (Join-Path $bin "NexusCore-windows-x86_64.exe")
)
$core = $coreCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $core) { throw "NexusCore.exe not found under $bin" }
$coreDest = Join-Path $bin "NexusCore.exe"
if ((Resolve-Path $core).Path -ne $coreDest) {
  Copy-Item -Force $core $coreDest
}
Copy-Item -Force $core (Join-Path $binaries "NexusCore-x86_64-pc-windows-msvc.exe")
Copy-Item -Force $core (Join-Path $binaries "NexusCore-x86_64-pc-windows-gnu.exe")

$appDir = Join-Path $NexusRoot "app"
Set-Location $appDir
Write-Host "npm install..."
npm install
if ($LASTEXITCODE -ne 0) { throw "npm install failed: $LASTEXITCODE" }

$cli = Join-Path $appDir "node_modules\@tauri-apps\cli\tauri.js"
if (-not (Test-Path $cli)) {
  throw "tauri CLI missing after npm install: $cli"
}

$stage = Join-Path $NexusRoot "app\src-tauri\ui-release-dist"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item (Join-Path $NexusRoot "app\ui\index.html") (Join-Path $stage "index.html")
if (Test-Path (Join-Path $NexusRoot "app\ui\assets")) {
  Copy-Item -Recurse (Join-Path $NexusRoot "app\ui\assets") (Join-Path $stage "assets")
}

$ov = Join-Path $NexusRoot "app\src-tauri\tauri.release-ui.json"
# Compile only — no NSIS/installer (user runs nexus.exe directly).
$json = '{"build":{"frontendDist":"./ui-release-dist"},"bundle":{"active":false}}'
[System.IO.File]::WriteAllText($ov, $json)

$env:NEXUS_CORE_BIN = (Resolve-Path $coreDest).Path
Write-Host "tauri build (no-bundle)..."
npx tauri build --no-bundle --config $ov
if ($LASTEXITCODE -ne 0) { throw "tauri build failed: $LASTEXITCODE" }

$exe = Join-Path $NexusRoot "app\src-tauri\target\release\nexus.exe"
if (-not (Test-Path $exe)) { throw "missing $exe" }
Write-Host "WIN_BUILD_OK"
Write-Host ("exe: " + $exe)
Get-Item $exe | Format-List FullName, Length, LastWriteTime
