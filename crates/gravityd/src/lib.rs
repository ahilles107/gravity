//! gravityd: always-on daemon that owns product state, durable delivery,
//! the scheduler, runtime adapters, and the control plane.

pub mod activity;
pub mod actor;
pub mod app;
pub mod backup;
pub mod botmgmt;
pub mod channel;
pub mod config;
pub mod db;
pub mod decisions;
pub mod delivery;
pub mod events;
pub mod home;
pub mod mcp;
pub mod messaging;
pub mod model;
pub mod overrides;
pub mod paths;
pub mod projectmgmt;
pub mod routine_validation;
pub mod runtime;
pub mod scheduler;
pub mod secrets;
pub mod server;
pub mod service;
pub mod supervisor;
pub mod terminal;
pub mod worktree;
pub mod ws;
