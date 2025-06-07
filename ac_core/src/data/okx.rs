use chrono::{Duration, Utc};
use data_center::sql::{QueryOption, query_bbo};
use futures::StreamExt;

use crate::{DataProvider, InstId};

use super::Bbo;

pub fn get_bbo_history_provider(
    instruments: Vec<InstId>,
    duration: Duration,
) -> impl DataProvider<Bbo> {
    let start = Utc::now() - duration;
    let query_option = QueryOption {
        instruments,
        start: Some(start),
        end: None,
    };
    let bbo_stream = query_bbo(query_option);
    let bbo_stream = bbo_stream.map(move |bbo| bbo.into());
    Box::pin(bbo_stream)
}
