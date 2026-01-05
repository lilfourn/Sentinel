//! Multi-Model File Analysis Pipeline
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  1. SCAN: Identify all files (PDFs, images, Office docs, text) │
//! │  2. EXTRACT: Pure Rust text extraction (pdf-extract, calamine) │
//! │  3. OPENAI WORKERS: GPT-5-nano (2-20 workers, 5 files/batch)   │
//! │  4. GROK SUMMARIZER: grok-4-1-fast (temp=0.1)                  │
//! │  5. GROK ORCHESTRATOR: Creates folder structure + assignments  │
//! │  6. EXECUTE: grok_execute_plan → WAL → Filesystem              │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

mod cache;
mod client;
mod explore_agent;
pub mod openai_worker;
mod orchestrator;
mod pdf_renderer;
mod summarizer;
mod utils;
mod vision;

pub mod document_parser;
pub mod integration;
pub mod types;

// Public API - used by commands/grok.rs and commands/ai.rs
pub use integration::{GrokOrganizer, ScanResult};
#[allow(unused_imports)]
pub use openai_worker::FileAnalysis;
pub use types::{
    sanitize_filename, sanitize_folder_path, AnalysisPhase, DocumentAnalysis, OrganizationPlan,
};
