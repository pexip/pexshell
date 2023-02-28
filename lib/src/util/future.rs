use futures::{future::join_all, Future};

pub async fn join_all_results<S: Iterator<Item = impl Future<Output = Result<T, E>>>, T, E>(
    iter: S,
) -> Result<Vec<T>, E> {
    let mut results = Vec::new();
    for r in join_all(iter).await {
        results.push(r?);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use std::{future::ready, sync::Arc, task::Poll};

    use parking_lot::Mutex;
    use test_helpers::future::MockFuture;

    use super::join_all_results;

    #[test]
    fn test_join_all_results_successful() {
        // Arrange
        let results = vec![ready::<Result<i32, ()>>(Ok(1)), ready(Ok(2)), ready(Ok(3))];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        // Act
        let result = runtime.block_on(join_all_results(results.iter().cloned()));

        // Assert
        assert_eq!(result, Ok(vec![1, 2, 3]));
    }

    #[test]
    fn test_join_all_results_fails() {
        fn create_mock_future(
            post: Arc<Mutex<Vec<usize>>>,
            id: usize,
            result: Result<i32, ()>,
        ) -> MockFuture<'static, Result<i32, ()>> {
            MockFuture::new(move |_ctx| {
                post.lock().push(id);
                Poll::Ready(result)
            })
        }

        // Arrange
        let call_order = Arc::new(Mutex::new(Vec::new()));

        let results = vec![
            create_mock_future(Arc::clone(&call_order), 0, Ok(1)),
            create_mock_future(Arc::clone(&call_order), 1, Err(())),
            create_mock_future(Arc::clone(&call_order), 2, Ok(3)),
        ];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        // Act
        let result = runtime.block_on(join_all_results(results.into_iter()));

        // Assert
        assert_eq!(result, Err(()));
        assert_eq!(*call_order.lock(), vec![0, 1, 2]);
    }
}
