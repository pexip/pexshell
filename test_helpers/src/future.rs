use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

type FutureFn<'a, T> = Box<dyn Fn(&mut Context<'_>) -> Poll<T> + 'a + Send + Sync>;

#[allow(clippy::module_name_repetitions)]
pub struct MockFuture<'a, T> {
    when_polled: FutureFn<'a, T>,
}

impl<'a, T> MockFuture<'a, T> {
    pub fn new(when_polled: impl Fn(&mut Context<'_>) -> Poll<T> + 'a + Send + Sync) -> Self {
        Self {
            when_polled: Box::new(when_polled),
        }
    }
}

impl<'a, T: 'static> Future for MockFuture<'a, T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (self.when_polled)(cx)
    }
}
