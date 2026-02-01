use futures::stream::FusedStream;

pub fn repeat_task_until_empty<T, Fut>(
    yield_next_task: impl (FnMut() -> Fut) + 'static,
) -> impl FusedStream<Item = T>
where
    Fut: Future<Output = Option<T>>,
{
    // impl-note: We need the stream to hold onto `yield_next_task` and repeatedly invoke it,
    //            so we decided to just juggle the task yielder throughout the unfolding operation.
    //              An alternative, cleaner-in-principle but much harder to read implementation is
    //            to mark `yield_next_task` as `mut` and write
    //                `futures::stream::unfold((), move |()| yield_next_task().map(|opt_t| opt_t.map(|t| (t, ()))))`.
    //            On one hand we need `yield_next_task` to stay in the *closure* passed to `unfold`, and on the other hand
    //            writing `move |()| async { yield_next_task().... }` would make the resulting Future
    //            borrow and leak `yield_next_task`, so to control the situation we need to use `futures::FutureExt::map`,
    //            which makes it a lot harder to read.
    futures::stream::unfold(yield_next_task, move |mut next| async {
        next().await.map(|v| (v, next))
    })
}
