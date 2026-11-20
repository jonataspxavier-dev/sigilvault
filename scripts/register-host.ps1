# Registra o host do Native Messaging (sigilvault-host.exe) para o usuário atual (HKCU), só para
# desenvolvimento. Na versão final, o instalador faz isso (fase 7).
# Registers the Native Messaging host (sigilvault-host.exe) for the current user (HKCU), for
# development only. In the final version, the installer does this (phase 7).
#
# Uso / Usage:
#   .\scripts\register-host.ps1                 # build de depuração / debug build (target\debug)
#   .\scripts\register-host.ps1 -Build release  # build final / release build (target\release)
#   .\scripts\register-host.ps1 -Unregister     # desfaz / undo
#
# Navegadores / Browsers: Chrome, Brave, Opera (lê a chave do Chrome / reads Chrome's key) e Firefox.

param(
  [ValidateSet("debug", "release")] [string] $Build = "debug",
  [switch] $Unregister
)

$ErrorActionPreference = "Stop"
$name = "app.sigilvault.host"
# Iguais a sigilvault_ipc::CHROMIUM_EXTENSION_ID e FIREFOX_EXTENSION_ID. / Same as those constants.
$chromiumId = "jclandjcanimgllfngognekhhhnomimf"
$firefoxId = "sigilvault@sigilvault.app"

$keys = @{
  chromium = @("HKCU:\Software\Google\Chrome\NativeMessagingHosts\$name", "HKCU:\Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\$name")
  firefox  = @("HKCU:\Software\Mozilla\NativeMessagingHosts\$name")
}

if ($Unregister) {
  foreach ($k in $keys.Values | ForEach-Object { $_ }) {
    if (Test-Path $k) { Remove-Item $k -Force; "removido / removed: $k" }
  }
  return
}

$dir = Join-Path (Split-Path $PSScriptRoot -Parent) "target\$Build"
$exe = Join-Path $dir "sigilvault-host.exe"
if (-not (Test-Path $exe)) { throw "Não encontrei / Not found: $exe  (rode / run: cargo build -p sigilvault-host$(if ($Build -eq 'release') { ' --release' }))" }

$manifests = @{
  chromium = [ordered]@{ name = $name; description = "SigilVault"; path = $exe; type = "stdio"; allowed_origins = @("chrome-extension://$chromiumId/") }
  firefox  = [ordered]@{ name = $name; description = "SigilVault"; path = $exe; type = "stdio"; allowed_extensions = @($firefoxId) }
}

foreach ($family in $manifests.Keys) {
  $file = Join-Path $dir "$name.$family.json"
  # UTF-8 sem BOM (o Chrome recusa BOM). / UTF-8 without BOM (Chrome rejects a BOM).
  [System.IO.File]::WriteAllText($file, ($manifests[$family] | ConvertTo-Json), [System.Text.UTF8Encoding]::new($false))
  foreach ($k in $keys[$family]) {
    New-Item -Path $k -Force | Out-Null
    Set-Item -Path $k -Value $file
    "registrado / registered: $k -> $file"
  }
}
