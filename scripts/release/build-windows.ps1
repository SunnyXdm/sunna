param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
# Experimental until the Windows host/driver changes have been committed.
cargo build --release -p sunnad -p sunna-update
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
New-Item -ItemType Directory -Force $Output | Out-Null
$stage = Join-Path ([IO.Path]::GetTempPath()) ('sunna-upd-' + [Guid]::NewGuid())
try {
  New-Item -ItemType Directory "$stage/bin" -Force | Out-Null
  Copy-Item target/release/sunnad.exe,target/release/sunna-update.exe "$stage/bin"
  Compress-Archive -Path "$stage/*" -DestinationPath "$Output/sunna-host-windows-x86_64.zip" -Force
  Copy-Item target/release/sunna-update.exe "$Output/sunna-update-windows-x86_64.exe"
} finally { Remove-Item -Recurse -Force $stage }
