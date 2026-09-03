//! Supervises the `edge-agent` child process and carries central's control orders to it
//! over a channel the agent's own MQTT event loop cannot take down with it.

pub mod child;
pub mod config;
pub mod control;
pub mod heartbeat;
pub mod supervisor;
