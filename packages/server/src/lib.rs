//! helpcore server — Axum HTTP server providing the REST + SSE chat API,
//! plugin runtime, conversation management, and multi-user auth.

#![warn(missing_docs)]

pub mod api;
pub mod auth;
pub mod config;
pub mod conversation;
pub mod db;
pub mod model;
pub mod plugins;
pub mod providers;
pub mod state;
pub mod web;
