#!/usr/bin/env bash
# Registra o host do Native Messaging (sigilvault-host) para o usuário atual, só para desenvolvimento.
# Nos pacotes .deb e .rpm, o instalador faz isso.
# Registers the Native Messaging host (sigilvault-host) for the current user, for development only.
# In the .deb and .rpm packages, the installer does this.
#
# Uso / Usage:
#   ./scripts/register-host.sh                 # build de depuração / debug build (target/debug)
#   ./scripts/register-host.sh release         # build final / release build (target/release)
#   ./scripts/register-host.sh --unregister    # desfaz / undo
#
# Só registra nos navegadores que já têm pasta de configuração (ou seja, que estão instalados).
# Only registers in browsers that already have a config folder (that is, that are installed).

set -euo pipefail

NAME="app.sigilvault.host"
# Iguais a sigilvault_ipc::CHROMIUM_EXTENSION_ID e FIREFOX_EXTENSION_ID. / Same as those constants.
CHROMIUM_ID="jclandjcanimgllfngognekhhhnomimf"
FIREFOX_ID="sigilvault@sigilvault.app"

CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
# família:pasta-do-navegador:pasta-dos-manifestos / family:browser folder:manifests folder
TARGETS=(
  "chromium:$CFG/google-chrome:$CFG/google-chrome/NativeMessagingHosts"
  "chromium:$CFG/chromium:$CFG/chromium/NativeMessagingHosts"
  "chromium:$CFG/BraveSoftware/Brave-Browser:$CFG/BraveSoftware/Brave-Browser/NativeMessagingHosts"
  "chromium:$CFG/microsoft-edge:$CFG/microsoft-edge/NativeMessagingHosts"
  "chromium:$CFG/vivaldi:$CFG/vivaldi/NativeMessagingHosts"
  "chromium:$CFG/opera:$CFG/opera/NativeMessagingHosts"
  "firefox:$HOME/.mozilla:$HOME/.mozilla/native-messaging-hosts"
)

mode="debug"
unregister=0
for arg in "$@"; do
  case "$arg" in
    debug|release) mode="$arg" ;;
    --unregister|-u) unregister=1 ;;
    *) echo "uso / usage: $0 [debug|release] [--unregister]" >&2; exit 2 ;;
  esac
done

if [ "$unregister" = 1 ]; then
  for entry in "${TARGETS[@]}"; do
    IFS=: read -r _family _browser dir <<<"$entry"
    if [ -f "$dir/$NAME.json" ]; then
      rm -- "$dir/$NAME.json"
      echo "removido / removed: $dir/$NAME.json"
    fi
  done
  exit 0
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exe="$root/target/$mode/sigilvault-host"
if [ ! -x "$exe" ] || [ ! -s "$exe" ]; then
  flag=""; [ "$mode" = release ] && flag=" --release"
  echo "Não encontrei / Not found: $exe  (rode / run: cargo build -p sigilvault-host$flag)" >&2
  exit 1
fi

manifest() { # $1 = família / family
  if [ "$1" = chromium ]; then
    printf '{\n  "name": "%s",\n  "description": "SigilVault",\n  "path": "%s",\n  "type": "stdio",\n  "allowed_origins": ["chrome-extension://%s/"]\n}\n' "$NAME" "$exe" "$CHROMIUM_ID"
  else
    printf '{\n  "name": "%s",\n  "description": "SigilVault",\n  "path": "%s",\n  "type": "stdio",\n  "allowed_extensions": ["%s"]\n}\n' "$NAME" "$exe" "$FIREFOX_ID"
  fi
}

found=0
for entry in "${TARGETS[@]}"; do
  IFS=: read -r family browser dir <<<"$entry"
  [ -d "$browser" ] || continue
  mkdir -p "$dir"
  manifest "$family" >"$dir/$NAME.json"
  echo "registrado / registered: $dir/$NAME.json -> $exe"
  found=1
done
[ "$found" = 1 ] || echo "Nenhum navegador encontrado / No browser found" >&2
