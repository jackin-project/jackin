use super::*;

#[tokio::test(flavor = "current_thread")]
async fn spawn_helpers_execute_on_current_thread_runtime() {
    assert_eq!(spawn_joined(async { 42 }).await.unwrap(), 42);
    let handle = spawn_stream("test.stream", std::future::pending::<()>());
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
}

#[test]
fn thread_helper_executes_work() {
    assert_eq!(thread_joined(|| 42).join().unwrap(), 42);
}
