use data_center::{
    self,
    sql::{QueryOption, query_bbo, query_level1},
    types::InstId,
};
use futures::{StreamExt, pin_mut};

// #[tokio::test]
async fn test_retrieve_bbo() {
    let query_option = QueryOption {
        instruments: vec![InstId::EthUsdtSwap],
        start: None,
        end: None,
    };
    let bbo_stream = query_bbo(query_option);

    pin_mut!(bbo_stream);
    let data = bbo_stream.next().await;
    assert!(dbg!(data).is_some());
}

// #[tokio::test]
async fn test_retrieve_level1() {
    let query_option = QueryOption {
        instruments: vec![InstId::EthUsdtSwap],
        start: None,
        end: None,
    };
    let level1_stream = query_level1(query_option);

    pin_mut!(level1_stream);
    let data = level1_stream.next().await;
    assert!(dbg!(data).is_some());
}
