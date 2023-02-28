use std::pin::Pin;

use futures::{executor::BlockingStream, Stream};

#[allow(clippy::module_name_repetitions)]
pub struct StreamWrapper<T> {
    inner: Pin<Box<dyn Stream<Item = T>>>,
}

impl<T> StreamWrapper<T> {
    #[must_use]
    pub fn new(inner: Pin<Box<dyn Stream<Item = T>>>) -> Self {
        Self { inner }
    }
}

impl<T> Stream for StreamWrapper<T> {
    type Item = T;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl<T> std::iter::IntoIterator for StreamWrapper<T> {
    type IntoIter = BlockingStream<Self>;
    type Item = T;

    fn into_iter(self) -> BlockingStream<Self> {
        futures::executor::block_on_stream(self)
    }
}
