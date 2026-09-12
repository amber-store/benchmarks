//! Object storage: a real S3 service, a measuring gateway in front of it, and
//! the client plumbing the backends use to reach it.

pub mod gateway;
pub mod s3;
pub mod service;
