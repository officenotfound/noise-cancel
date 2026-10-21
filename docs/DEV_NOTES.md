# Developer notes

## Type-check the Windows code on a Mac (no Windows needed)
```sh
mkdir -p /tmp/qmbin && cat > /tmp/qmbin/msvc-lib <<'SH'
#!/bin/sh
# MSVC lib.exe-style shim: lib -out:X -nologo a.o b.o  ->  ar crs X a.o b.o
out=""; files=""
for a in "$@"; do case "$a" in -out:*|/out:*) out="${a#*:}";; -nologo|/nologo) ;; *) files="$files $a";; esac; done
rm -f "$out"; exec ar crs "$out" $files
SH
chmod +x /tmp/qmbin/msvc-lib
rustup target add x86_64-pc-windows-msvc
export CC_x86_64_pc_windows_msvc=clang AR_x86_64_pc_windows_msvc=/tmp/qmbin/msvc-lib
cargo check --target x86_64-pc-windows-msvc -p qm-app      # compiles everything incl. tract's assembler kernels
```
Checking only; linking an `.exe` needs the MSVC toolchain, so real builds come from GitHub Actions (`.github/workflows/build.yml`).

## Run the Windows build on a Mac with Parallels
- The VM is Windows 11 **ARM64**: x64 builds run under emulation (functionally fine, timings pessimistic, not representative of an Intel PC).
- Parallels may suspend the VM when idle: `prlctl resume "Windows 11"` first.
- Run commands: `prlctl exec "Windows 11" cmd /c "..."`.
- Copy files in (no shared folders, no network; the macOS firewall blocks the VM from reaching a Mac-side web server):
  `scripts/vm_put.sh <local file> 'C:\path\file'` (streams over `prlctl exec` stdin, verifies SHA-256, retries).
- Get the CI build: `gh run download <run-id> -n QuietMic-windows -D /tmp/qm` then copy the files in with `vm_put.sh`.
- The VM's microphone is the Mac's microphone (needs macOS microphone permission for Parallels). For an audio check
  without VB-Cable use `QuietMic.exe --selftest 6` (writes `%APPDATA%\QuietMic\selftest.txt` and WAVs).

## Architecture (Windows)
`qm-win` (WASAPI capture/render threads, lock-free ring, drift-compensating resampler, device listing, VB-Cable detection)
-> `qm-app` (supervisor thread: calibration, restart on failure/device change; tray menu; settings TOML; log file)
-> `qm-engine` (presets, reframer, gate, limiter, governor) -> `qm-dsp` (models).
