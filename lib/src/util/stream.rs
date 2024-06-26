use std::pin::Pin;

use futures::{executor::BlockingStream, Stream};

#[allow(clippy::module_name_repetitions)]
pub struct StreamWrapper<'a, T> {
    inner: Pin<Box<dyn Stream<Item = T> + Send + 'a>>,
}

impl<'a, T> StreamWrapper<'a, T> {
    #[must_use]
    pub fn new(inner: Pin<Box<dyn Stream<Item = T> + Send + 'a>>) -> Self {
        Self { inner }
    }
}

impl<'a, T> Stream for StreamWrapper<'a, T> {
    type Item = T;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl<'a, T> std::iter::IntoIterator for StreamWrapper<'a, T> {
    type IntoIter = BlockingStream<Self>;
    type Item = T;

    fn into_iter(self) -> BlockingStream<Self> {
        futures::executor::block_on_stream(self)
    }
}
