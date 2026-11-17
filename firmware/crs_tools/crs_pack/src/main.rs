// crs-pack/src/main.rs
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crs_format::*;
use crs_pack::{dump_crs_file, make_slot_image_v1, pack_toml_bytes};
use std::{fs, path::PathBuf};

#[derive(Parser)]
#[command(name = "crs-pack")]
#[command(about = "CRS pack/dump/validate tool (host-only)")]
#[command(
    long_about = "CRS pack/dump/validate tool (host-only)\n\nUSAGE EXAMPLES:\n  crs-pack pack --in cfg.toml --out cfg.crs\n  crs-pack validate --in cfg.toml\n  crs-pack dump --in cfg.crs\n  crs-pack dump --in cfg.crs --toml\n  crs-pack dump --in cfg.crs --fw-compat-id 0x20260216"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    #[command(
        about = "Parse TOML, validate strictly, emit CRS binary",
        long_about = "Parse TOML, validate strictly, emit CRS binary\n\nSYNTAX:\n  crs-pack pack --in <TOML_FILE> --out <CRS_FILE> [--build-id <U32>]\n\nEXAMPLES:\n  crs-pack pack --in cfg.toml --out cfg.crs\n  crs-pack pack --in cfg.toml --out cfg.crs --build-id 305419896"
    )]
    Pack {
        #[arg(
            long,
            value_name = "TOML_FILE",
            help = "Input TOML file (use: --in <TOML_FILE>)"
        )]
        r#in: PathBuf,
        #[arg(
            long,
            value_name = "CRS_FILE",
            help = "Output CRS binary file (use: --out <CRS_FILE>)"
        )]
        out: PathBuf,
        /// Tool-generated identifier stored in header.build_id (u32).
        /// If omitted, tool derives one deterministically from TOML bytes (CRC32).
        #[arg(
            long,
            value_name = "U32",
            help = "Override header.build_id (decimal u32). If omitted, derived from TOML bytes."
        )]
        build_id: Option<u32>,
    },

    #[command(
        about = "Parse TOML, validate strictly, emit a flashable slot image (SlotHeader + CRS + padding)",
        long_about = "Parse TOML, validate strictly, emit a flashable slot image (SlotHeader + CRS + padding)\n\nSYNTAX:\n  crs-pack pack-slot --in <TOML_FILE> --out <SLOT_BIN> --slot-size <BYTES> [--build-id <U32>] [--seq <U32>] [--uncommitted]\n\nNOTES:\n  - --slot-size must match the device's erase unit (page/sector) size.\n\nEXAMPLES:\n  crs-pack pack-slot --in cfg.toml --out slot_a.bin --slot-size 4096\n  crs-pack pack-slot --in cfg.toml --out slot_a.bin --slot-size 4096 --seq 1"
    )]
    PackSlot {
        #[arg(
            long,
            value_name = "TOML_FILE",
            help = "Input TOML file (use: --in <TOML_FILE>)"
        )]
        r#in: PathBuf,
        #[arg(
            long,
            value_name = "SLOT_BIN",
            help = "Output slot image file (use: --out <SLOT_BIN>)"
        )]
        out: PathBuf,
        /// Tool-generated identifier stored in CRS header.build_id (u32).
        /// If omitted, tool derives one deterministically from TOML bytes (CRC32).
        #[arg(
            long,
            value_name = "U32",
            help = "Override CRS header.build_id (decimal u32). If omitted, derived from TOML bytes."
        )]
        build_id: Option<u32>,
        /// Monotonic sequence counter used for A/B slot selection (higher is newer).
        /// If omitted, defaults to current unix time in seconds (u32).
        #[arg(
            long,
            value_name = "U32",
            help = "Slot header seq (decimal u32). If omitted, uses current unix time."
        )]
        seq: Option<u32>,
        /// Slot size in bytes (must be >= 32 + 288).
        /// Must match the device erase unit size (page/sector).
        #[arg(long, value_name = "BYTES")]
        slot_size: usize,
        /// Emit an uncommitted slot header (committed=0xFFFF_FFFF) for power-loss-safe workflows.
        #[arg(long)]
        uncommitted: bool,
    },

    #[command(
        about = "Parse TOML and validate strictly (no output)",
        long_about = "Parse TOML and validate strictly. No output file; exit 0 on success.\n\nSYNTAX:\n  crs-pack validate --in <TOML_FILE>\n\nEXAMPLES:\n  crs-pack validate --in cfg.toml"
    )]
    Validate {
        #[arg(
            long,
            value_name = "TOML_FILE",
            help = "Input TOML file (use: --in <TOML_FILE>)"
        )]
        r#in: PathBuf,
    },

    #[command(
        about = "Validate CRS binary and print summary or canonical TOML",
        long_about = "Parse CRS binary, validate header+CRC, print summary or canonical TOML\n\nSYNTAX:\n  crs-pack dump --in <CRS_FILE> [--toml] [--fw-compat-id <HEX_U32>]\n\nEXAMPLES:\n  crs-pack dump --in cfg.crs\n  crs-pack dump --in cfg.crs --toml\n  crs-pack dump --in cfg.crs --fw-compat-id 0x20260216"
    )]
    Dump {
        #[arg(
            long,
            value_name = "CRS_FILE",
            help = "Input CRS binary file (use: --in <CRS_FILE>)"
        )]
        r#in: PathBuf,
        /// Emit canonical TOML representation
        #[arg(long)]
        toml: bool,
        /// Optional: validate fw_compat_id matches this value (hex like 0x20260216)
        #[arg(long, value_name = "HEX_U32")]
        fw_compat_id: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Pack {
            r#in,
            out,
            build_id,
        } => {
            let toml_bytes =
                fs::read(&r#in).with_context(|| format!("read TOML {}", r#in.display()))?;
            let bytes = pack_toml_bytes(&toml_bytes, build_id)?;
            fs::write(&out, bytes).with_context(|| format!("write {}", out.display()))?;

            let (hdr, _payload) =
                crs_format::parse_and_validate_basic(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
            let hv: HeaderView = hdr.into();
            println!(
                "ok: wrote {} bytes -> {} (fw=0x{:08X} schema={} len={} crc=0x{:08X} build_id=0x{:08X})",
                TOTAL_LEN,
                out.display(),
                hv.fw_compat_id,
                hv.crs_schema_version,
                hv.len,
                hv.crc32,
                hv.build_id
            );
        }

        Cmd::PackSlot {
            r#in,
            out,
            build_id,
            seq,
            slot_size,
            uncommitted,
        } => {
            let toml_bytes =
                fs::read(&r#in).with_context(|| format!("read TOML {}", r#in.display()))?;
            let crs_bytes = pack_toml_bytes(&toml_bytes, build_id)?;

            let seq = match seq {
                Some(v) => v,
                None => {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default();
                    now.as_secs() as u32
                }
            };

            let slot_bytes = make_slot_image_v1(&crs_bytes, seq, !uncommitted, slot_size)?;
            fs::write(&out, slot_bytes).with_context(|| format!("write {}", out.display()))?;

            println!(
                "ok: wrote slot image -> {} (slot_size={} seq={} committed={})",
                out.display(),
                slot_size,
                seq,
                if uncommitted { "false" } else { "true" }
            );
        }

        Cmd::Validate { r#in } => {
            let toml_bytes =
                fs::read(&r#in).with_context(|| format!("read TOML {}", r#in.display()))?;
            let _ = crs_pack::build_payload_from_toml_bytes(&toml_bytes)?;
            println!("ok: {}", r#in.display());
        }

        Cmd::Dump {
            r#in,
            toml: as_toml,
            fw_compat_id,
        } => {
            let s = dump_crs_file(&r#in, as_toml, fw_compat_id.as_deref())?;
            print!("{}", s);
        }
    }

    Ok(())
}
