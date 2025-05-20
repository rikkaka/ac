use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::main]
async fn main() {
    // OKX WebSocket 服务器地址
    let url = "wss://ws.okx.com:8443/ws/v5/public";

    println!("Connecting to {}", url);

    // 连接 WebSocket
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("WebSocket connection established");

    let (mut write, mut read) = ws_stream.split();

    // 发送 ping 消息
    write
        .send(Message::Text("ping".into()))
        .await
        .expect("Failed to send ping");
    println!("Ping message sent");

    // 读取一条消息（可选）
    if let Some(msg) = read.next().await {
        if matches!(msg, Ok(ref m) if *m == Message::text("pong")) {
            println!("Received pong");
        }
    } else {
        println!("No response received");
    }
}
