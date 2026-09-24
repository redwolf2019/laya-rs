use super::*;
use tokio::{net::TcpListener, sync::oneshot, time::timeout};

async fn wait_queue(client: &scheduler::Client, count: usize) {
    timeout(Duration::from_secs(2), async {
        while client.snapshot().waiting != count {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn tcp_disconnect_cancels_waiters_but_does_not_stop_cpu() {
    let (mut scheduler, release, mut starts) = blocked_worker(1);
    let client = scheduler.client();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(client.clone(), Limits::default());
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                stopped.await.unwrap();
            })
            .await
            .unwrap();
    });
    let running = http::post(address, VALID).await;
    starts.recv().await.unwrap();
    let queued = http::post(address, VALID).await;
    wait_queue(&client, 1).await;
    drop(queued);
    wait_queue(&client, 0).await;
    drop(running);
    assert_eq!(client.snapshot().inflight, 1);
    assert_eq!(http::get(address, "/readyz").await.0, 200);
    assert!(starts.try_recv().is_err());
    client.close();
    release.send(()).unwrap();
    scheduler.run().await.unwrap();
    assert_eq!(
        (
            client.snapshot().waiting,
            client.snapshot().inflight,
            client.snapshot().tracked_tasks
        ),
        (0, 0, 0)
    );
    stop.send(()).unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn chunked_http_overflow_returns_before_terminal_chunk() {
    let mut scheduler = Scheduler::test_worker(1, 0, |_| unreachable!());
    let client = scheduler.client();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = router(
        client.clone(),
        Limits {
            max_body_bytes: 8,
            ..Limits::default()
        },
    );
    let (stop, stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                stopped.await.unwrap();
            })
            .await
            .unwrap();
    });
    let socket = http::send(address, b"POST /v1/system-one HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n1234\r\n5\r\n56789\r\n").await;
    let (status, _, body) = http::receive(socket).await;
    assert_eq!(status, 413);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"]["code"],
        "payload_too_large"
    );
    client.close();
    scheduler.run().await.unwrap();
    stop.send(()).unwrap();
    server.await.unwrap();
}
