//! Authentication and Author network transport.
//!
//! The workspace dependency contract lets this crate depend on
//! `slingshot-configuration`, `slingshot-agent-protocol`, and
//! `slingshot-domain`. This commit declares the crate's authentication family
//! and its transport-policy leaf as documentation-only structure.

pub mod artifact_download;
pub mod authentication;
pub mod author_cross_site_request_forgery_protection;
pub mod author_hypertext_transfer_protocol_policy;
pub mod capability_discovery;
pub mod command_submission;
mod connection_phase;
pub mod event_stream_heartbeat;
pub mod event_stream_reconnection;
pub mod job_event_reducer;
pub mod job_snapshot_reconciliation;
pub mod request_authentication;
pub mod server_sent_event_decoder;
pub mod structured_job_result;
pub mod terminal_failure;
pub mod selected_author_exchange;
pub mod selected_author_authenticated_read;
pub mod selected_author_http;
pub mod selected_author_http2_frames;
pub mod selected_author_http2;
pub mod selected_author_http2_artifact;
pub mod selected_author_http2_events;
pub mod selected_author_events;
pub mod event_stream_reset;
pub mod physical_job_missing;
pub mod subscription_high_water;
pub mod selected_author_http1_events;
pub mod selected_author_http2_handshake;
pub mod selected_author_http2_headers;
pub mod selected_author_http2_request;
pub mod selected_author_http2_flow;
pub mod selected_author_http2_response;
mod selected_author_hpack_codes;
pub mod selected_author_hpack_huffman;
pub mod selected_author_hpack_integer;
pub mod selected_author_hpack_string;
pub mod selected_author_hpack_table;
pub mod selected_author_hpack_block;
pub mod selected_author_lookup;
pub mod selected_author_submission;
pub mod selected_author_transport;
pub mod subscription_event_fold;
pub mod transport_policy;
