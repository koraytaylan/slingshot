//! Decisions inside joined asynchronous work count toward the enclosing function.

async fn joined_work(condition: bool) {
    tokio::join!(
        async {
            if condition {}
            if condition {}
            if condition {}
            if condition {}
            if condition {}
            if condition {}
            if condition {}
            if condition {}
            if condition {}
            Ok::<(), ()>(())
        },
        async { Ok::<(), ()>(()) },
    );
}
