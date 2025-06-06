use anyhow::Result;
use futures::{Sink, Stream, ready};
use pin_project::pin_project;
use std::collections::VecDeque;
use std::fmt::Display;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::Interval;
use tokio_tungstenite::tungstenite::{self, Message};
use utils::Duplex;

#[macro_export]
macro_rules! delegate_sink {
    ($field:ident, $item:ty) => {
        fn poll_ready(
            self: core::pin::Pin<&mut Self>,
            cx: &mut core::task::Context<'_>,
        ) -> core::task::Poll<Result<(), Self::Error>> {
            self.project().$field.poll_ready(cx)
        }
        fn start_send(self: core::pin::Pin<&mut Self>, item: $item) -> Result<(), Self::Error> {
            self.project().$field.start_send(item)
        }
        fn poll_flush(
            self: core::pin::Pin<&mut Self>,
            cx: &mut core::task::Context<'_>,
        ) -> core::task::Poll<Result<(), Self::Error>> {
            self.project().$field.poll_flush(cx)
        }
        fn poll_close(
            self: core::pin::Pin<&mut Self>,
            cx: &mut core::task::Context<'_>,
        ) -> core::task::Poll<Result<(), Self::Error>> {
            self.project().$field.poll_close(cx)
        }
    };
}

/// 实现底层流的心跳机制。在给定时间未接收到新消息后发送 ping 消消息，并注册需要接收 pong 消息。若未在给定时间内收到pong，发出错误。
#[pin_project]
pub struct Heartbeat<S> {
    #[pin]
    inner: S,
    ping_ticker: Interval,
    pong_timer: Interval,
    is_waiting_pong: bool,
    is_started: bool,
}

impl<S> Heartbeat<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    pub fn new(inner: S, interval: Duration, pong_timeout: Duration) -> Self {
        let ticker = tokio::time::interval(interval);
        let pong_timer = tokio::time::interval(pong_timeout);
        Self {
            inner,
            ping_ticker: ticker,
            pong_timer,
            is_waiting_pong: false,
            is_started: false,
        }
    }
}

impl<S> Sink<Message> for Heartbeat<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    type Error = tungstenite::Error;

    delegate_sink!(inner, Message);
}

impl<S> Stream for Heartbeat<S>
where
    S: Duplex<Message, tungstenite::Error, Result<Message, tungstenite::Error>>,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        // 初次调用
        if !*this.is_started {
            this.ping_ticker.reset();
            *this.is_started = true;
        }

        // 1. 若没有在给定时间内收到pong，则关闭
        if *this.is_waiting_pong && this.pong_timer.poll_tick(cx).is_ready() {
            tracing::error!("Heartbeat timeout");
            return Poll::Ready(None);
        }

        // 2. 若距离上次收到消息的时间到达心跳间隔，则发送ping消息并注册计时器
        if this.ping_ticker.poll_tick(cx).is_ready() {
            tracing::debug!("Sending ping");
            if let Err(e) = this.inner.as_mut().start_send("ping".into()) {
                tracing::error!("Failed to send heartbeat: {e}");
                return Poll::Ready(None);
            }
            let _ = this.inner.as_mut().poll_flush(cx)?;

            // 发送ping消息后，注册pong计时器
            *this.is_waiting_pong = true;
            this.pong_timer.reset();
            // 将pong计时器注册到当前上下文
            if this.pong_timer.poll_tick(cx).is_ready() {
                tracing::error!("The duration of pong timer is zero");
                return Poll::Ready(None);
            }
        }

        // 3. 接收消息
        let msg = loop {
            let Some(msg) = ready!(this.inner.as_mut().poll_next(cx)) else {
                return Poll::Ready(None);
            };
            // 在收到任意消息后，重置心跳计时器
            this.ping_ticker.reset();
            // 并且结束等待pong
            *this.is_waiting_pong = false;

            if matches!(msg, Ok(ref m) if *m == Message::text("pong")) {
                tracing::debug!("Received pong");
            } else {
                break msg;
            }
        };

        // 4. 返回数据
        Poll::Ready(Some(msg))
    }
}

/// Auto reconnect when the inner Stream returns a None or the inner Sink returns an Error
#[pin_project(project = AutoReconeectProj)]
pub struct AutoReconnect<MkConn, Fut, S, I> {
    make_conn: MkConn,
    #[pin]
    conn_future: Option<Fut>,
    #[pin]
    curr_conn: Option<S>,
    sink_buf: VecDeque<I>,
}

impl<MkConn, Fut, S, I> AutoReconnect<MkConn, Fut, S, I>
where
    MkConn: FnMut() -> Fut,
    Fut: Future<Output = Result<S>>,
{
    pub async fn new(mut make_connection: MkConn) -> Result<Self> {
        let inner = make_connection().await?;
        Ok(Self {
            make_conn: make_connection,
            conn_future: None,
            curr_conn: Some(inner),
            sink_buf: VecDeque::new(),
        })
    }
}

impl<MkConn, Fut, S, I, E> AutoReconeectProj<'_, MkConn, Fut, S, I>
where
    MkConn: FnMut() -> Fut,
    Fut: Future<Output = Result<S, E>>,
    E: Display,
{
    fn close_conn_and_set_conn_future(&mut self) {
        tracing::info!("Reconnecting");
        self.curr_conn.set(None);
        self.conn_future.set(Some((self.make_conn)()));
    }

    fn poll_set_conn(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        loop {
            let conn_res = ready!(self.conn_future.as_mut().as_pin_mut().unwrap().poll(cx));
            match conn_res {
                Ok(conn) => {
                    self.curr_conn.set(Some(conn));

                    tracing::info!("Reconnected");
                    return Poll::Ready(());
                }
                Err(e) => {
                    tracing::error!("Error reconnecting: {e}");
                    self.close_conn_and_set_conn_future();
                }
            }
        }
    }
}

impl<MkConn, Fut, S, I, E> Stream for AutoReconnect<MkConn, Fut, S, I>
where
    MkConn: FnMut() -> Fut,
    Fut: Future<Output = Result<S, E>>,
    S: Stream,
    E: Display,
{
    type Item = <S as Stream>::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        loop {
            if let Some(conn) = this.curr_conn.as_mut().as_pin_mut() {
                match conn.poll_next(cx) {
                    Poll::Ready(Some(msg)) => return Poll::Ready(Some(msg)),
                    Poll::Ready(None) => {
                        this.close_conn_and_set_conn_future();
                    }
                    Poll::Pending => return Poll::Pending,
                }
            } else {
                ready!(this.poll_set_conn(cx));
            }
        }
    }
}

// 在返回错误与否取决于原conn是否返回错误，以使send的可靠性与原conn一致。
impl<MkConn, Fut, S, I, E> Sink<I> for AutoReconnect<MkConn, Fut, S, I>
where
    MkConn: FnMut() -> Fut,
    Fut: Future<Output = Result<S, E>>,
    S: Sink<I>,
    I: Clone,
    E: Display,
    <S as Sink<I>>::Error: Display,
{
    type Error = <S as Sink<I>>::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = self.project();

        loop {
            if let Some(conn) = this.curr_conn.as_mut().as_pin_mut() {
                return conn.poll_ready(cx);
            } else {
                // 若连接尚未建立，则Pending。若连接已建立，进入下次循环并返回conn的poll_ready
                ready!(this.poll_set_conn(cx));
            }
        }
    }

    fn start_send(self: Pin<&mut Self>, item: I) -> Result<(), Self::Error> {
        let this = self.project();
        this.sink_buf.push_back(item);

        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let mut this = self.project();

        'outer: loop {
            if let Some(mut conn) = this.curr_conn.as_mut().as_pin_mut() {
                // 若连接存在，则遍历并让conn start_send sink_buf中的消息
                while let Some(item) = this.sink_buf.pop_front() {
                    // 若start_send出错，则断开连接并准备重新建立
                    if let Err(e) = conn.as_mut().start_send(item.clone()) {
                        tracing::error!("Error sending message: {e}");
                        this.sink_buf.push_back(item);
                        this.close_conn_and_set_conn_future();
                        continue 'outer;
                    }
                }
                return conn.poll_flush(cx);
            } else {
                // 若连接尚未建立，则Pending。若连接已建立，进入下次循环继续send消息。
                ready!(this.poll_set_conn(cx));
            }
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        if let Some(conn) = this.curr_conn.as_pin_mut() {
            ready!(conn.poll_close(cx))?;
        }
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt, pin_mut, stream};

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    // A simple duplex stream for testing
    #[pin_project::pin_project]
    struct TestDuplex {
        #[pin]
        rx: ReceiverStream<Message>,
        #[pin]
        tx: mpsc::Sender<Message>,
    }

    impl Stream for TestDuplex {
        type Item = Result<Message, tungstenite::Error>;
        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let this = self.project();
            match this.rx.poll_next(cx) {
                Poll::Ready(Some(msg)) => Poll::Ready(Some(Ok(msg))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl Sink<Message> for TestDuplex {
        type Error = tungstenite::Error;
        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn start_send(self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
            let this = self.project();
            this.tx
                .try_send(item)
                .map_err(|_| tungstenite::Error::ConnectionClosed)
        }
        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_heartbeat() {
        let (server_tx, client_rx) = mpsc::channel(10);
        let (client_tx, mut server_rx) = mpsc::channel(10);

        let duplex = TestDuplex {
            rx: ReceiverStream::new(client_rx),
            tx: client_tx,
        };

        let mut hb = Heartbeat::new(duplex, Duration::from_millis(50), Duration::from_millis(10));

        // Client
        let client = tokio::spawn(async move {
            assert!(matches!(hb.next().await, Some(Ok(ref m)) if *m == Message::text("1")));
            assert!(matches!(hb.next().await, Some(Ok(ref m)) if *m == Message::text("2")));
            assert!(matches!(hb.next().await, Some(Ok(ref m)) if *m == Message::text("3")));
            assert!((hb.next().await).is_none());
        });

        // Server
        let server = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            server_tx.send(Message::text("1")).await.unwrap();

            let ping_msg = server_rx.recv().await;
            assert_eq!(ping_msg, Some(Message::text("ping")));

            server_tx.send(Message::text("pong")).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            server_tx.send(Message::text("2")).await.unwrap();

            tokio::time::sleep(Duration::from_millis(50)).await;
            server_tx.send(Message::text("3")).await.unwrap();

            let ping_msg = server_rx.recv().await;
            assert_eq!(ping_msg, Some(Message::text("ping")));
            // 等待超时，client应关闭
            tokio::time::sleep(Duration::from_millis(80)).await;
            server_tx.send(Message::text("4")).await.unwrap_err();
        });

        client.await.unwrap();
        server.await.unwrap();
    }

    // Simple test message for AutoReconnect testing
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct TestMsg(u32);

    // Test connection that can be controlled to fail on demand
    #[pin_project]
    struct TestConnection {
        #[pin]
        rx: ReceiverStream<TestMsg>,
        tx: mpsc::Sender<TestMsg>,
        max_sends: usize, // Maximum number of messages before failure
        send_count: usize,
    }

    impl Stream for TestConnection {
        type Item = TestMsg;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            self.project().rx.poll_next(cx)
        }
    }

    impl Sink<TestMsg> for TestConnection {
        type Error = String;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: TestMsg) -> Result<(), Self::Error> {
            let this = self.project();

            // Check if connection should fail
            if this.send_count >= this.max_sends {
                return Err("Connection failed".to_string());
            }

            *this.send_count += 1;
            this.tx
                .try_send(item)
                .map_err(|e| format!("Send error: {e}"))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_auto_reconnect_sink() {
        // Track connection count
        let connect_count = Arc::new(AtomicUsize::new(0));

        // Store received messages
        let received_msgs = Arc::new(Mutex::new(Vec::new()));

        let connect_count_clone = connect_count.clone();
        let received_msgs_clone = received_msgs.clone();

        // Connection factory
        let make_connection = move || {
            connect_count_clone.fetch_add(1, Ordering::SeqCst);
            let (client_tx, mut client_rx) = mpsc::channel(10);
            let (_, server_rx) = mpsc::channel(10);

            // First connection fails after 3 messages
            let max_sends = 3;

            // Create connection
            let conn = TestConnection {
                rx: ReceiverStream::new(server_rx),
                tx: client_tx,
                max_sends,
                send_count: 0,
            };

            // Handle received messages
            let received = received_msgs_clone.clone();
            tokio::spawn(async move {
                while let Some(msg) = client_rx.recv().await {
                    received.lock().unwrap().push(msg);
                }
            });

            async move { Ok::<_, anyhow::Error>(conn) }
        };

        // Create AutoReconnect
        let auto_reconn = AutoReconnect::new(make_connection).await.unwrap();
        pin_mut!(auto_reconn);

        // Send 10 messages
        for i in 1..=10 {
            auto_reconn.send(TestMsg(i)).await.unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify reconnection happened
        assert_eq!(connect_count.load(Ordering::SeqCst), 4);

        // Verify all messages were received
        let msgs = received_msgs.lock().unwrap();
        assert_eq!(msgs.len(), 10);
        for i in 1..=10 {
            assert!(msgs.contains(&TestMsg(i)));
        }
    }

    #[tokio::test]
    async fn test_auto_reconnect_stream() {
        // Connection factory
        let make_connection = move || {
            let iter = vec![1, 2, 3].into_iter();
            let stream = stream::iter(iter);

            async move { Ok::<_, anyhow::Error>(stream) }
        };

        // Create AutoReconnect
        let auto_conn: AutoReconnect<_, _, _, ()> =
            AutoReconnect::new(make_connection).await.unwrap();
        pin_mut!(auto_conn);

        // Receive messages
        for i in 1..=3 {
            assert_eq!(auto_conn.next().await.unwrap(), i);
        }

        // Auto reconnect. Receive messages again
        for i in 1..=3 {
            assert_eq!(auto_conn.next().await.unwrap(), i);
        }
    }
}
