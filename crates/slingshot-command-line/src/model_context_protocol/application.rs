//! Composing the transport, the revisions, and the services into one server.
//!
//! Assembly lives apart from the pieces it assembles: which revision answered,
//! which tool ran, and which resource was read are decisions owned elsewhere, and
//! this owns the wiring that makes exactly one of each happen per request.
