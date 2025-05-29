use chrono::{Duration, Utc};
use data_center::{self, sql::{query_bbo, QueryOption}};
use futures::{pin_mut, StreamExt};

#[tokio::test]
async fn test_retrieve_bbo() {
    let query_option = QueryOption {
        instruments: vec!["ETH-USDT-SWAP".try_into().unwrap()],
        start: Some(Utc::now() - Duration::seconds(100)),
        end: None
    };
    let bbo_stream = query_bbo(query_option);
    pin_mut!(bbo_stream);
    let data = bbo_stream.next().await;
    assert!(dbg!(data).is_some());
    while let Some(data) = bbo_stream.next().await {
        dbg!(data);
    }
}