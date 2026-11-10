//! One-shot import of Cloud Router's legacy `downloads` site-settings document
//! into the Deploy download catalog tables.
//!
//! The legacy document was a nested JSON blob authored by the Cloud Router
//! console: `{ product, cards: [{ ..., actions: [...] }] }`. Deploy models the
//! same facts as a catalog (one row) plus cards plus one item per
//! platform/architecture installer. This crate only maps the document; the
//! binary in `main.rs` applies the plan through the repository port.
//!
//! The mapping is deliberately lossless for the facts Deploy owns and
//! lossless-*aware* for the facts it does not: legacy actions carry no update
//! configuration (the feed was configured elsewhere), so imported installers
//! start with no feed and the report says how many need one before publishing.

pub mod legacy;

pub use legacy::{plan_import, CardPlan, ImportOptions, ImportPlan, ItemPlan};
