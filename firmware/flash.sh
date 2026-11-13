#!/usr/bin/env sh
# Cargo runner: waits for the RP2040 bootloader drive, mounts it if the
# desktop didn't, then flashes the ELF passed by cargo.
set -eu
export PATH="$HOME/.cargo/bin:$PATH"

dev=/dev/disk/by-label/RPI-RP2
here=$(cd "$(dirname "$0")" && pwd)

# If Macropad OS firmware is running, ask it to enter the bootloader.
if [ ! -e "$dev" ]; then
    if (cd "$here/.." && cargo run -q -p mpcfg -- bootloader) 2>/dev/null; then
        :
    fi
fi

tries=0
while [ ! -e "$dev" ]; do
    if [ "$tries" -eq 0 ]; then
        echo "Waiting for RPI-RP2 (hold knob + keys 10 and 12 to enter the bootloader)..."
    fi
    tries=$((tries + 1))
    if [ "$tries" -gt 300 ]; then
        echo "Timed out after 30 s waiting for RPI-RP2" >&2
        exit 1
    fi
    sleep 0.1
done

if ! findmnt -n -S "$(readlink -f "$dev")" >/dev/null 2>&1; then
    udisksctl mount --no-user-interaction -b "$dev" >/dev/null
fi

exec elf2uf2-rs -d "$@"
