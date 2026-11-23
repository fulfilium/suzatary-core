//! What the box answers from itself: documents on disk replaced whole (`docs`) and the stores
//! kept as them (uploads, blobs, texts and their keys, the upload index, requests, pairing,
//! epochs, frontiers, the triage's state and pulse, health registers), a PDF's text
//! (`pdftotext`), the system clock, the manuals and tailscaled's whois.

#![forbid(unsafe_code)]

pub mod attachments;
pub mod blobs;
pub mod clock;
pub mod docs;
pub mod epochs;
pub mod frontiers;
pub mod health_file;
pub mod manuals;
pub mod model_health;
pub mod pairing;
pub mod pdftotext;
pub mod pulse;
pub mod requests;
pub mod source_health;
pub mod state;
pub mod tapped;
pub mod text_keys;
pub mod texts;
pub mod upload_index;
pub mod whois;
