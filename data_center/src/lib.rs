
pub mod instruments_profile;
pub mod okx_api;
pub mod sql;
pub mod types;
mod utils;

// 解析订阅并建立连接，推送数据。还可接收写入以发送消息。
// 推送的是可以直接拿去用的Data。
pub struct Terminal<HS, WS> {
    history_stream: HS,
    ws_stream: WS,
}

// impl<HS, WS> Terminal<HS, WS> where HS: Stream<Item = Data
