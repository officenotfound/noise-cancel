#!/bin/sh
# Copy a local file into the Parallels Windows VM by streaming it over `prlctl exec` stdin (no shared folders or
# network needed), verify SHA-256, retry on the occasional prlctl hiccup.
# Usage: scripts/vm_put.sh <local file> <C:\remote\path> [vm name]
set -u
SRC=$1; DST=$2; VM=${3:-Windows 11}
want=$(shasum -a 256 "$SRC" | cut -c1-16)
for try in 1 2 3; do
  cat "$SRC" | prlctl exec "$VM" powershell -NoProfile -Command "\$d=Split-Path '$DST'; New-Item -ItemType Directory -Force \$d | Out-Null; \$i=[Console]::OpenStandardInput(); \$o=[IO.File]::Create('$DST'); \$i.CopyTo(\$o); \$o.Close()" >/dev/null 2>&1
  got=$(prlctl exec "$VM" powershell -NoProfile -Command "if (Test-Path '$DST') { (Get-FileHash '$DST' -Algorithm SHA256).Hash.Substring(0,16).ToLower() }" 2>/dev/null | tr -d '\r')
  if [ "$want" = "$got" ]; then printf 'ok   %s\n' "$DST"; exit 0; fi
  printf 'retry %s (attempt %s)\n' "$DST" "$try" >&2
done
printf 'FAILED %s\n' "$DST" >&2; exit 1
