use futures::{Sink, Stream, ready};
use pin_project::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::Interval;
use tokio_tungstenite::tungstenite::{self, Message};

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

pub trait Duplex<I, IE, O>: Sink<I, Error = IE> + Stream<Item = O> + Unpin {}
impl<T, I, IE, O> Duplex<I, IE, O> for T where T: Sink<I, Error = IE> + Stream<Item = O> + Unpin {}

/// 实现底层流的心跳机制。在给定时间未接收到新消息后发送 ping 消息，并注册需要接收 pong 消息。若未在给定时间内收到pong，发出错误。
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

        // 初次调用时，将计时器注册到上下文中
        if !*this.is_started {
            let _ = this.pong_timer.poll_tick(cx);
            let _ = this.ping_ticker.poll_tick(cx);
            this.ping_ticker.reset();
            *this.is_started = true;
        }

        // 1. 若没有在给定时间内收到pong，则关闭
        if *this.is_waiting_pong && this.pong_timer.poll_tick(cx).is_ready()  {
            tracing::error!("Heartbeat timeout");
            return Poll::Ready(None);
        }

        // 2. 若距离上次收到消息的时间到达心跳间隔，则发送ping消息并注册计时器
        if this.ping_ticker.poll_tick(cx).is_ready() {
            if let Err(e) = this.inner.as_mut().start_send("ping".into()) {
                tracing::error!("Failed to send heartbeat: {e}");
                return Poll::Ready(None);
            }
            *this.is_waiting_pong = true;
            this.pong_timer.reset();
        }

        // 3. 接收消息
        let msg = loop {
            let Some(msg) = ready!(this.inner.as_mut().poll_next(cx)) else {
                return Poll::Ready(None);
            };
            // 在收到任意消息后，重置心跳计时器
            this.ping_ticker.reset();
            // 如果是pong，则结束等待pong
            if matches!(msg, Ok(ref m) if *m == Message::text("pong")) {
                dbg!("Received pong");
                *this.is_waiting_pong = false;
            } else {
                break msg;
            }
        };

        // 4. 返回数据
        Poll::Ready(Some(msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
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
            assert!(matches!(hb.next().await, None));
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
            tokio::time::sleep(Duration::from_millis(20)).await;
            server_tx.send(Message::text("4")).await.unwrap_err();
        });

        client.await.unwrap();
        server.await.unwrap();
    }
}
