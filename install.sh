#!/usr/bin/env bash
set -euo pipefail

REPO="uplate/uplate"
INSTALLER_URL="https://github.com/${REPO}/releases/latest/download/uplate-installer.sh"

if command -v curl >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -LsSf "$INSTALLER_URL" | sh
elif command -v wget >/dev/null 2>&1; then
  wget -qO- "$INSTALLER_URL" | sh
else
  echo "curl or wget is required to install uplate" >&2
  exit 1
fi
