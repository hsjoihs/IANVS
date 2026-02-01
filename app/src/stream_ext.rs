use futures::stream::FusedStream;

pub fn repeat_task_until_empty<T, Fut>(
    yield_next_task: impl (FnMut() -> Fut) + 'static,
) -> impl FusedStream<Item = T>
where
    Fut: Future<Output = Option<T>>,
{
    // note: We need the stream to hold onto `yield_next_task` and repeatedly invoke it,
    //       so just juggle the task yielder throughout the unfolding operation.
    futures::stream::unfold(yield_next_task, move |mut next| async {
        next().await.map(|v| (v, next))
    })
}
