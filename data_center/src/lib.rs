pub mod instruments_profile;
pub mod okx_api;
pub mod sql;
pub mod types;
mod terminal;
mod utils;

use once_cell::sync::Lazy;
use serde::Deserialize;
use smartstring::alias::String;

pub use types::{Data, OrderPush, Action};
pub use terminal::Terminal;

static CONFIG: Lazy<Config> = Lazy::new(|| {
    dotenvy::dotenv_override()
        .expect("Please set PG_HOST in the .env or the environment variables");
    envy::from_env::<Config>().unwrap()
});

#[derive(Deserialize, Debug)]
struct Config {
    pg_host: String,
    api_key: String,
    secret_key: String,
    passphrase: String,
    heartbeat_interval: u64,
    heartbeat_timeout: u64,
}

#[cfg(test)]
mod test {
    use crate::CONFIG;

    #[test]
    fn test_env_vars() {
        dbg!(&*CONFIG);
    }
}
