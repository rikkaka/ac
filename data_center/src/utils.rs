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
