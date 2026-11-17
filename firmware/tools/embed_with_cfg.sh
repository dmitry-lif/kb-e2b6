#!/usr/bin/env bash
set -euo pipefail

# Flash CRS config slots.  (probe-rs 0.32.x)
#
# Layout is selected by the firmware build (board_* feature) and emitted into:
#   kb_firmware/target/crs_artifacts/flash_layout.env
#
# This script packs an input TOML with `crs-pack pack-slot`, composes the full
# reserved flash region (LED journal page + Slot A + Slot B) into ONE image, and
# writes it in a single verified probe-rs download. One erase/program/verify
# pass — no fragile multi-write sequence that can leave the board half-written.
#
# SAFETY: before touching flash it reads FLASH_OPTR and REFUSES to proceed unless
# the MCU is in SINGLE bank (option byte DBANK=0): firmware and config flashing support
# single bank only, and flashing a dual-bank chip can leave it half-written.
# The check is read-only. Set single bank once per chip with ./flash_set_single_bank.sh
# (why: firmware/docs/recovery.md section 2).

usage() {
  cat <<'EOF'
Usage:
  ./embed_with_cfg.sh --toml ./config.toml
  ./embed_with_cfg.sh --toml ./config.toml --slot A
  ./embed_with_cfg.sh --toml ./config.toml --slot B

Notes:
  - This script does NOT flash firmware.
  - It flashes cfg slots using `probe-rs`.
  - It always resets the target after flashing.
  - This script supports ONLY: probe-rs 0.32.x
  - It uses the generated flash layout env file.
  - SAFETY: aborts unless the MCU is single-bank (DBANK=0). Run
    ./flash_set_single_bank.sh once per new chip first.
  - Policy: composes the whole reserved region (LED journal + Slot A + Slot B)
    into ONE image and writes it in a single verified download:
    - the chosen slot holds the packed config
    - the other slot + the LED journal page are zero-filled (guaranteed erased,
      so no stale committed config or LED record can survive)
  - The packed slot always gets seq 1: the other slot is zero-filled (invalid), so seq does not
    matter for boot selection; WebHID saves then continue with 2, 3, ...
  - Env knobs: KB_SKIP_BANK_CHECK=1 (bypass preflight, use only if certain),
    KB_CONNECT_UNDER_RESET=1 (attach under reset for a hung board).
  - It does NOT build tools. `crs-pack` must already exist.
    - Preferred (repo-local): `../crs_tools/target/release/crs-pack`
    - Or (debug): `../crs_tools/target/debug/crs-pack`
    - Or on PATH as `crs-pack`
    - Or set `CRS_PACK_BIN=/path/to/crs-pack`
  - Ensure the flash layout env exists first (build firmware once):
    - `cd ../kb_firmware && cargo build --release`
  - This script must be run from `tools/`.
EOF
}

# Flags shared by every probe-rs invocation (probe-rs 0.32.x).
PROBE_RS_COMMON=(--non-interactive)
if [[ "${KB_CONNECT_UNDER_RESET:-0}" == "1" ]]; then
  PROBE_RS_COMMON+=(--connect-under-reset)
fi

# Read one 32-bit word from target memory. Read-only; safe on any board.
probe_rs_read_u32() {
  local chip="$1" addr="$2"
  probe-rs read "${PROBE_RS_COMMON[@]}" --chip "$chip" b32 "$addr" 1
}

# Refuse to continue unless the MCU is in SINGLE bank (FLASH_OPTR.DBANK == 0).
# STM32G4 FLASH_OPTR = 0x4002_2020, DBANK = bit 22 (0x0040_0000).
# 1 = dual bank -> abort before any write.
require_single_bank() {
  local chip="$1"
  if [[ "${KB_SKIP_BANK_CHECK:-0}" == "1" ]]; then
    echo "warn: KB_SKIP_BANK_CHECK=1 — skipping single-bank preflight" >&2
    return 0
  fi

  local raw
  if ! raw="$(probe_rs_read_u32 "$chip" 0x40022020)"; then
    echo "error: could not read FLASH_OPTR (0x40022020) to verify bank mode" >&2
    echo "hint: is a probe attached and the board powered?" >&2
    echo "hint: if you are CERTAIN the board is single-bank: re-run with KB_SKIP_BANK_CHECK=1" >&2
    exit 1
  fi

  # probe-rs may print '<addr>: <val>' or just '<val>'. Drop the address token,
  # take the remaining 32-bit hex value.
  local optr
  optr="$(printf '%s\n' "$raw" | tr 'A-F' 'a-f' | tr -s ' \t:' '\n' \
    | sed 's/^0x//' | grep -E '^[0-9a-f]{1,8}$' | grep -vx '40022020' | tail -n1)" || true
  if [[ -z "$optr" ]]; then
    echo "error: could not parse FLASH_OPTR from probe-rs output:" >&2
    printf '%s\n' "$raw" >&2
    echo "hint: if CERTAIN the board is single-bank: re-run with KB_SKIP_BANK_CHECK=1" >&2
    exit 1
  fi

  if (( (0x$optr & 0x00400000) != 0 )); then
    echo "error: MCU is in DUAL-bank mode (FLASH_OPTR=0x$optr, DBANK=1)." >&2
    echo "       Firmware and config flashing support single bank (DBANK=0) only." >&2
    echo "       Refusing to touch the board." >&2
    echo "fix: run ./flash_set_single_bank.sh once (switches to single bank, erases the chip)," >&2
    echo "     flash the firmware, then retry. Why: firmware/docs/recovery.md section 2" >&2
    exit 1
  fi
  echo "ok: single-bank confirmed (FLASH_OPTR=0x$optr, DBANK=0)"
}

# Flash a raw bin at an absolute address, read back to verify, then reset.
# probe-rs 0.32 does download + --verify + --reset in one invocation.
probe_rs_flash_verify_reset() {
  local chip="$1" base_addr="$2" path="$3"
  probe-rs download "${PROBE_RS_COMMON[@]}" \
    --chip "$chip" \
    --binary-format bin --base-address "$base_addr" \
    --verify --reset \
    "$path"
}

CFG_TOML=""
SLOT="A"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --toml)
      CFG_TOML="${2:-}"
      shift 2
      ;;
    --slot)
      SLOT="${2:-}"
      shift 2
      ;;
    *)
      echo "error: unknown arg '$1'" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$CFG_TOML" ]]; then
  echo "error: must provide --toml" >&2
  usage >&2
  exit 2
fi

if [[ "$SLOT" != "A" && "$SLOT" != "B" && "$SLOT" != "a" && "$SLOT" != "b" ]]; then
  echo "error: --slot must be 'A' or 'B' (got '$SLOT')" >&2
  exit 2
fi

SLOT="${SLOT^^}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# This script is intentionally strict to avoid path confusion.
# Run it from its own directory (`tools`).
if [[ "$(pwd)" != "$SCRIPT_DIR" ]]; then
  echo "error: run this script from: $SCRIPT_DIR" >&2
  echo "hint: cd \"$SCRIPT_DIR\" && ./embed_with_cfg.sh --toml <file.toml>" >&2
  exit 2
fi

ARTIFACT_DIR="../kb_firmware/target/crs_artifacts"

LAYOUT_ENV="$ARTIFACT_DIR/flash_layout.env"
if [[ ! -f "$LAYOUT_ENV" ]]; then
  echo "error: missing $LAYOUT_ENV (build.rs should generate it)" >&2
  echo "hint: run: cd ../kb_firmware && cargo build --release" >&2
  exit 1
fi

# shellcheck disable=SC1090
source "$LAYOUT_ENV"

if [[ -z "${PROBE_RS_CHIP:-}" \
   || -z "${CFG_SLOT_A_BASE:-}" || -z "${CFG_SLOT_B_BASE:-}" || -z "${CFG_SLOT_SIZE:-}" \
   || -z "${LED_STATE_FLASH_PAGE_BASE:-}" || -z "${LED_STATE_FLASH_PAGE_SIZE:-}" ]]; then
  echo "error: $LAYOUT_ENV missing required variables" >&2
  exit 1
fi

if [[ ! -f "$CFG_TOML" ]]; then
  echo "error: TOML not found: $CFG_TOML" >&2
  exit 1
fi

CRS_PACK_BIN="${CRS_PACK_BIN:-}"
if [[ -z "$CRS_PACK_BIN" ]]; then
  if [[ -x "../crs_tools/target/release/crs-pack" ]]; then
    CRS_PACK_BIN="../crs_tools/target/release/crs-pack"
  elif [[ -x "../crs_tools/target/debug/crs-pack" ]]; then
    CRS_PACK_BIN="../crs_tools/target/debug/crs-pack"
  elif command -v crs-pack >/dev/null 2>&1; then
    CRS_PACK_BIN="$(command -v crs-pack)"
  fi
fi

if [[ -z "$CRS_PACK_BIN" || ! -x "$CRS_PACK_BIN" ]]; then
  echo "error: crs-pack not found (or not executable)" >&2
  echo "hint: build it: cd ../crs_tools && cargo build -p crs-pack --release" >&2
  echo "hint: expected either: ../crs_tools/target/{release,debug}/crs-pack or crs-pack on PATH" >&2
  echo "hint: or set: CRS_PACK_BIN=/abs/path/to/crs-pack" >&2
  exit 1
fi

OUT_DIR="$ARTIFACT_DIR"
mkdir -p "$OUT_DIR"

OUT_SLOT_BIN="$OUT_DIR/slot_${SLOT}_${CFG_SLOT_SIZE}.bin"
COMBINED_BIN="$OUT_DIR/cfg_region_${SLOT}.bin"

# The reserved region must be contiguous and ascending: LED page, Slot A, Slot B,
# ending exactly at FLASH_END. We flash the whole region as ONE image so the
# write is a single erase/program/verify pass.
REGION_BASE="$LED_STATE_FLASH_PAGE_BASE"
if (( (LED_STATE_FLASH_PAGE_BASE + LED_STATE_FLASH_PAGE_SIZE) != CFG_SLOT_A_BASE )) \
   || (( (CFG_SLOT_A_BASE + CFG_SLOT_SIZE) != CFG_SLOT_B_BASE )) \
   || (( (CFG_SLOT_B_BASE + CFG_SLOT_SIZE) != FLASH_END )); then
  echo "error: reserved flash region is not contiguous (LED|A|B|END); refusing to flash" >&2
  echo "  LED=$LED_STATE_FLASH_PAGE_BASE (+$LED_STATE_FLASH_PAGE_SIZE) A=$CFG_SLOT_A_BASE B=$CFG_SLOT_B_BASE END=$FLASH_END" >&2
  exit 1
fi
REGION_LEN=$(( LED_STATE_FLASH_PAGE_SIZE + 2 * CFG_SLOT_SIZE ))

# The other slot is zero-filled below (invalid), so the packed slot is the only candidate.
SEQ=1

# 1) Pack TOML -> slot image (SlotHeader + CRS @0x20 + padding to slot size).
"$CRS_PACK_BIN" pack-slot --in "$CFG_TOML" --out "$OUT_SLOT_BIN" --slot-size "$CFG_SLOT_SIZE" --seq "$SEQ"

SLOT_LEN=$(stat -c%s "$OUT_SLOT_BIN")
if (( SLOT_LEN != CFG_SLOT_SIZE )); then
  echo "error: packed slot size mismatch: $SLOT_LEN != $CFG_SLOT_SIZE" >&2
  exit 1
fi

# 2) Compose the full reserved-region image: [LED=zeros][Slot A][Slot B].
# The non-target slot and the LED journal page are zero-filled (not 0xFF) so they
# are guaranteed to be erased+programmed and no stale data can survive.
: > "$COMBINED_BIN"
dd if=/dev/zero bs=1 count="$LED_STATE_FLASH_PAGE_SIZE" status=none >> "$COMBINED_BIN"
if [[ "$SLOT" == "A" ]]; then
  cat "$OUT_SLOT_BIN" >> "$COMBINED_BIN"                                   # Slot A = config
  dd if=/dev/zero bs=1 count="$CFG_SLOT_SIZE" status=none >> "$COMBINED_BIN" # Slot B = wiped
else
  dd if=/dev/zero bs=1 count="$CFG_SLOT_SIZE" status=none >> "$COMBINED_BIN" # Slot A = wiped
  cat "$OUT_SLOT_BIN" >> "$COMBINED_BIN"                                   # Slot B = config
fi

REGION_ACTUAL=$(stat -c%s "$COMBINED_BIN")
if (( REGION_ACTUAL != REGION_LEN )); then
  echo "error: composed region size mismatch: $REGION_ACTUAL != $REGION_LEN" >&2
  exit 1
fi

# 3) Preflight: refuse to touch the board unless it is single-bank (DBANK=0).
require_single_bank "$PROBE_RS_CHIP"

# 4) One atomic, verified flash of the whole region, then reset.
probe_rs_flash_verify_reset "$PROBE_RS_CHIP" "$REGION_BASE" "$COMBINED_BIN"

echo "ok: flashed cfg slot='$SLOT' board='${KB_BOARD:-unknown}' chip='$PROBE_RS_CHIP' region='$REGION_BASE'+${REGION_LEN}B toml='$CFG_TOML' seq='$SEQ'"

