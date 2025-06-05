use std::{fs, path::Path};

use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::types::InstId;

pub static INSTRUMENT_PROFILES: Lazy<InstrumentProfiles> = Lazy::new(|| {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let profiles_path = Path::new(manifest_dir).join("instrument_profiles.toml");
    let profiles_str = fs::read_to_string(profiles_path).unwrap();
    toml::from_str(&profiles_str).unwrap()
});

type InstrumentProfiles = FxHashMap<InstId, InstrumentProfile>;

#[derive(Deserialize, Debug)]
pub struct InstrumentProfile {
    pub size_scale: f64,
    pub size_digits: i32,
    pub price_digits: i32,
}

#[cfg(test)]
mod test {
    use crate::instruments_profile::INSTRUMENT_PROFILES;

    #[test]
    fn test() {
        assert!(INSTRUMENT_PROFILES.len() > 0);
        dbg!(&INSTRUMENT_PROFILES);
    }
}
