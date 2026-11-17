#!/usr/bin/env bash
# Check the STM32G474CB flash bank mode and switch it to single bank (DBANK=0) if needed.
# Mandatory once per new chip, before flashing firmware or configuration: chips leave the
# factory in dual-bank mode. Why, and what to do if a board is already stuck:
# firmware/docs/recovery.md §2. Uses probe-rs only.
set -euo pipefail

CHIP="STM32G474CBTx"
PR=(--non-interactive --chip "$CHIP")

FLASH_KEYR=0x40022008
FLASH_OPTKEYR=0x4002200C
FLASH_SR=0x40022010
FLASH_CR=0x40022014
FLASH_OPTR=0x40022020
DBANK=0x00400000                      # FLASH_OPTR bit 22

# Read one 32-bit register; print it as hex digits (empty on failure).
rd() {
  local a
  a="$(printf '%s' "${1#0x}" | tr 'A-F' 'a-f')"
  probe-rs read "${PR[@]}" b32 "$1" 1 2>/dev/null | tr 'A-F' 'a-f' | tr -s ' \t:' '\n' \
    | sed 's/^0x//' | grep -E '^[0-9a-f]{1,8}$' | grep -vx "$a" | tail -n1 || true
}
wr() { probe-rs write "${PR[@]}" b32 "$1" "$2" >/dev/null; }

optr="$(rd $FLASH_OPTR)"
if [[ -z "$optr" ]]; then
  echo "error: could not read FLASH_OPTR (probe attached? board powered from USB?)" >&2
  exit 1
fi
echo "FLASH_OPTR = 0x$optr  DBANK=$(( (0x$optr & DBANK) != 0 ))"

if (( (0x$optr & DBANK) == 0 )); then
  echo "OK: single bank. Nothing changed."
  exit 0
fi

# The low byte is the read protection level: it must stay 0xAA (level 0). Never touch anything else.
if (( (0x$optr & 0xFF) != 0xAA )); then
  echo "error: read protection is not level 0 (low byte is not aa). Option bytes left untouched." >&2
  exit 1
fi

new="$(printf '0x%08X' $(( 0x$optr & ~DBANK & 0xFFFFFFFF )))"
echo "dual bank: switching to single bank (FLASH_OPTR -> $new)"
wr $FLASH_SR 0x0000C3FB               # clear old error flags
wr $FLASH_KEYR 0x45670123             # unlock flash control
wr $FLASH_KEYR 0xCDEF89AB
wr $FLASH_OPTKEYR 0x08192A3B          # unlock option bytes
wr $FLASH_OPTKEYR 0x4C5D6E7F
cr="$(rd $FLASH_CR)"
if [[ -z "$cr" ]] || (( (0x$cr >> 30) & 3 )); then
  echo "error: unlock failed (FLASH_CR=0x${cr:-?}). Nothing was changed." >&2
  exit 1
fi
wr $FLASH_OPTR "$new"                 # DBANK cleared, all other bits unchanged
wr $FLASH_CR 0x00020000               # OPTSTRT: program the option bytes
wr $FLASH_CR 0x08000000 || true       # OBL_LAUNCH: reload them; the MCU resets, an error here is normal
sleep 1

optr="$(rd $FLASH_OPTR)"
if [[ -z "$optr" ]] || (( (0x$optr & DBANK) != 0 )); then
  echo "error: DBANK is still set (FLASH_OPTR=0x${optr:-?}). See firmware/docs/recovery.md §2." >&2
  exit 1
fi
echo "FLASH_OPTR = 0x$optr  DBANK=0"
echo "erasing the whole flash (the old contents don't survive the switch)"
probe-rs erase "${PR[@]}" >/dev/null
echo "OK: single bank, flash erased. Flash the firmware next."
