//! Literal route assertions from Plan 0005, independent of the fake's table.

#[test]
fn product_routes_match_the_pinned_author_contract() {
    use slingshot_agent_connection::{
        artifact_download, author_cross_site_request_forgery_protection, event_stream_reconnection,
        job_snapshot_reconciliation,
    };
    assert_eq!(
        author_cross_site_request_forgery_protection::TOKEN_ROUTE,
        "/libs/granite/csrf/token.json"
    );
    assert_eq!(event_stream_reconnection::EVENT_ROUTE, "/bin/slingshot-agent/events");
    assert_eq!(job_snapshot_reconciliation::LOOKUP_ROUTE, "/bin/slingshot-agent/operations/lookup");
    assert_eq!(
        job_snapshot_reconciliation::PHYSICAL_JOB_ROUTE,
        "/bin/slingshot-agent/jobs/snapshot"
    );
    assert_eq!(
        job_snapshot_reconciliation::HIGH_WATER_ROUTE,
        "/bin/slingshot-agent/events/high-water"
    );
    assert_eq!(
        artifact_download::artifact_route(
            "https://author.example/aem",
            "operation-one",
            "content_package"
        )
        .unwrap(),
        "https://author.example/aem/bin/slingshot-agent/operations/operation-one/artifacts/content_package"
    );
}
