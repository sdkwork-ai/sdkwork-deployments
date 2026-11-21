//! Direct-mode `miniprogram-ci` process execution.
//!
//! The implementation lives in the service crate so both the worker and the
//! service host share one sandbox and one executable policy; this module keeps
//! the worker's historical import path stable.
pub use sdkwork_intelligence_deploy_service::TokioChannelCiProcess;
