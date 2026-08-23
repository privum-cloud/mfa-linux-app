#!/usr/bin/env bash
# Install the current Tessera .deb on an operator machine over SSH.
#
#   SSHPASS='...' ./scripts/deploy-app.sh sysadmin@192.168.1.241
#
# The password is read from the environment and never written down here: this
# file is in git and the machines it targets are reachable from the LAN.
set -euo pipefail

TARGET="${1:-}"
if [[ -z "$TARGET" ]]; then
  echo "usage: SSHPASS='<password>' $0 user@host" >&2
  exit 64
fi
if [[ -z "${SSHPASS:-}" ]]; then
  echo "set SSHPASS in the environment" >&2
  exit 64
fi

DEB=$(ls -t src-tauri/target/release/bundle/deb/Tessera_*_amd64.deb 2>/dev/null | head -1)
if [[ -z "$DEB" ]]; then
  echo "no .deb found — run: npm run tauri build -- --bundles deb" >&2
  exit 66
fi

NAME=$(basename "$DEB")
echo "sending $NAME to $TARGET"
sshpass -e scp -o StrictHostKeyChecking=no "$DEB" "$TARGET:/tmp/$NAME"

# apt-get resolves the webkit and gtk dependencies; dpkg -i would not.
sshpass -e ssh -o StrictHostKeyChecking=no "$TARGET" \
  "echo \"\$SSHPASS\" | sudo -S apt-get install -y /tmp/$NAME >/dev/null 2>&1; \
   rm -f /tmp/$NAME; dpkg -l tessera | tail -1"

echo "done"
