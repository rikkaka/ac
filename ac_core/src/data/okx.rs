use chrono::Duration;
use data_center::sql::BboQuerier;
use futures::{Stream, StreamExt, TryStreamExt};

use crate::{DataProvider, InstId};

use super::Bbo;

pub fn get_bbo_history_stream(inst_id: InstId, duration: Duration) -> impl Stream<Item = Bbo> {
    let bbo_querier = BboQuerier::new()
        .with_instrument_id(inst_id)
        .with_latest(duration);

    let bbo_stream = bbo_querier.query();

    bbo_stream.map(move |bbo| bbo.into())
}
