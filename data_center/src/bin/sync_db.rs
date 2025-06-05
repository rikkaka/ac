use data_center::sql::POOL;
use futures::future::join_all;
use sqlx::Executor;

const REMOTE_SCHEMA: &str = "remote_import";
const TABLES: &[&str] = &["okx_bbo", "okx_trades"];

#[tokio::main]
async fn main() {
    let mut join_handles = vec![];
    for table in TABLES {
        let handle = tokio::spawn(sync_table(table));
        join_handles.push(handle);
    }
    join_all(join_handles).await;
}

async fn sync_table(table_name: &str) {
    let remote_table_name = format!("{REMOTE_SCHEMA}.{table_name}");

    let sql = format!("SELECT MAX(ts) FROM {table_name}");
    let max_ts: Option<i64> = sqlx::query_scalar(&sql).fetch_one(&*POOL).await.ok();

    let mut sql = format!("INSERT INTO {table_name} SELECT * FROM {remote_table_name}");
    if let Some(max_ts) = max_ts {
        sql.push_str(&format!(" where ts > {max_ts}"));
    }
    sql.push_str(" ON CONFLICT DO NOTHING");
    POOL.execute(sqlx::query(&sql)).await.unwrap();
}
