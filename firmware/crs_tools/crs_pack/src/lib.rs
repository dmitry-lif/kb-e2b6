//! Library for CRS TOML packing and CRS binary dumping/validation.
//!
//! Keep this crate small and deterministic; the CLI in `main.rs` is intentionally thin.

mod dump;
mod encode;
mod slot;
mod toml_model;
mod util;

pub use dump::{dump_crs_bytes, dump_crs_file};
pub use encode::{build_payload_from_toml_bytes, derive_build_id_from_toml_bytes, pack_toml_bytes};
pub use slot::make_slot_image_v1;
pub use util::{parse_hex_u32, PackInput};

#[cfg(test)]
mod tests {
    use super::*;
    use crs_format::{parse_and_validate_basic, payload_bytes};

    #[test]
    fn pack_dump_toml_pack_roundtrips_payload() {
        let toml0 = include_bytes!("../../testdata/example.toml");
        let crs0 = pack_toml_bytes(toml0, Some(0x11111111)).expect("pack 0");
        let (_h0, p0) = parse_and_validate_basic(&crs0).expect("parse 0");

        let dumped = dump_crs_bytes(&crs0, true, None).expect("dump --toml");
        let crs1 = pack_toml_bytes(dumped.as_bytes(), Some(0x22222222)).expect("pack 1");
        let (_h1, p1) = parse_and_validate_basic(&crs1).expect("parse 1");

        assert_eq!(payload_bytes(&p0), payload_bytes(&p1));
    }
}
