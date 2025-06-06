use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum InstId {
    #[default]
    EthUsdtSwap,
    BtcUsdtSwap,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Trades,
    BboTbt,
    Orders,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum InstType {
    Swap,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum TdMode {
    Cross,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum OrdType {
    Limit,
    Market,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum OrderState {
    Canceled,
    Live,
    PartiallyFilled,
    Filled,
}
#[derive(Deserialize, Clone, Debug)]
pub enum ExecType {
    T,
    M,
}