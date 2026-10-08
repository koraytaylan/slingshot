//! Counter outcomes, cancellation, saturation and socket ownership checks.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, advance};

const PHASE_MILLISECONDS: u64 = 7;
const CANCEL_MILLISECONDS: u64 = 11;
const EXPECTED_NANOSECONDS: u64 = 25_000_000;
const EXPECTED_STARTED: u64 = 3;
const SOCKET_TIMEOUT_SECONDS: u64 = 5;

#[tokio::test(start_paused = true)]
async fn outcomes_include_failure_and_abandonment_without_changing_results() {
    let counters = PhaseCounters::default();
    let untouched = observe_with(&counters, async { Ok::<_, ()>(()) });
    drop(untouched);
    assert_eq!(counters.snapshot().started, 0);
    for success in [true, false] {
        let expected = if success { Ok("kept") } else { Err("unchanged") };
        assert_eq!(
            observe_with(&counters, async {
                advance(Duration::from_millis(PHASE_MILLISECONDS)).await;
                expected
            })
            .await,
            expected
        );
    }
    let pending = observe_with(&counters, std::future::pending::<Result<(), ()>>());
    assert!(
        tokio::time::timeout(Duration::from_millis(CANCEL_MILLISECONDS), pending).await.is_err()
    );
    let snapshot = counters.snapshot();
    assert_eq!(snapshot.started, EXPECTED_STARTED);
    assert_eq!(snapshot.succeeded, 1);
    assert_eq!(snapshot.failed, 1);
    assert_eq!(snapshot.abandoned, 1);
    assert_eq!(snapshot.elapsed_nanoseconds, EXPECTED_NANOSECONDS);
}

#[test]
fn counters_saturate_and_routes_hold_distinct_storage() {
    let counter = AtomicU64::new(u64::MAX - 1);
    add(&counter, u64::MAX);
    add(&counter, 1);
    assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    assert!(!std::ptr::eq(Route::Author.counters(), Route::IdentityManagement.counters()));
    let snapshot = Route::Author.snapshot();
    assert_eq!(snapshot.phases.len(), snapshot.phase_names.len());
}

#[tokio::test]
async fn raw_socket_counts_only_transferred_bytes_and_closes_on_drop() {
    static COUNTERS: std::sync::LazyLock<Counters> = std::sync::LazyLock::new(Counters::default);
    const REQUEST: &[u8] = b"private request bytes";
    const RESPONSE: &[u8] = b"private response bytes";
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).await.unwrap();
    let (mut peer, _) = listener.accept().await.unwrap();
    let mut observed = ObservedSocket { socket, counters: &COUNTERS };
    let exchange = async {
        observed.write_all(REQUEST).await.unwrap();
        observed.flush().await.unwrap();
        let mut request = vec![0; REQUEST.len()];
        peer.read_exact(&mut request).await.unwrap();
        assert_eq!(request, REQUEST);
        peer.write_all(RESPONSE).await.unwrap();
        let mut response = vec![0; RESPONSE.len()];
        observed.read_exact(&mut response).await.unwrap();
        assert_eq!(response, RESPONSE);
        assert_eq!(COUNTERS.received.load(Ordering::Relaxed), RESPONSE.len() as u64);
        assert_eq!(COUNTERS.transmitted.load(Ordering::Relaxed), REQUEST.len() as u64);
        assert_eq!(format!("{observed:?}"), "ObservedSocket([redacted])");
        drop(observed);
        assert_eq!(peer.read(&mut [0]).await.unwrap(), 0);
        assert_eq!(COUNTERS.dropped.load(Ordering::Relaxed), 1);
    };
    tokio::time::timeout(Duration::from_secs(SOCKET_TIMEOUT_SECONDS), exchange).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn response_transitions_finish_once_and_cancellation_stays_distinct() {
    static COUNTERS: std::sync::LazyLock<Counters> = std::sync::LazyLock::new(Counters::default);
    let response = observe_response_with(&COUNTERS, async |observation| {
        advance(Duration::from_millis(PHASE_MILLISECONDS)).await;
        observation.head_complete();
        observation.head_complete();
        advance(Duration::from_millis(PHASE_MILLISECONDS)).await;
        Err::<(), _>("body failure")
    })
    .await;
    assert_eq!(response, Err("body failure"));
    let head = &COUNTERS.phases[Phase::ResponseHead as usize];
    let body = &COUNTERS.phases[Phase::ResponseBody as usize];
    assert_eq!(head.snapshot().started, 1);
    assert_eq!(head.snapshot().succeeded, 1);
    assert_eq!(body.snapshot().started, 1);
    assert_eq!(body.snapshot().failed, 1);
    let abandoned = observe_response_with(&COUNTERS, async |observation| {
        observation.head_complete();
        std::future::pending::<Result<(), ()>>().await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(CANCEL_MILLISECONDS), abandoned).await.is_err()
    );
    assert_eq!(body.snapshot().abandoned, 1);
    let expected = Duration::from_millis(PHASE_MILLISECONDS + CANCEL_MILLISECONDS).as_nanos();
    assert_eq!(u128::from(body.snapshot().elapsed_nanoseconds), expected);
}
