$ErrorActionPreference = "Stop"
$cargoHome = Join-Path $env:USERPROFILE ".cargo\bin"
$env:Path = "$cargoHome;$env:Path"
if (Get-Command cargo -ErrorAction SilentlyContinue) {
  cargo -V
  exit 0
}
Write-Host "INSTALLING_RUST"
$init = Join-Path $env:TEMP "rustup-init.exe"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Invoke-WebRequest -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile $init
Start-Process -FilePath $init -ArgumentList "-y","--default-toolchain","stable","--profile","minimal" -Wait -NoNewWindow
$env:Path = "$cargoHome;$env:Path"
if (-not (Test-Path (Join-Path $cargoHome "cargo.exe"))) {
  throw "cargo.exe missing after rustup"
}
& (Join-Path $cargoHome "cargo.exe") -V
Write-Host "RUST_OK"
