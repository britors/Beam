//! Core RDP session engine for Beam.
//!
//! This crate has no GTK (or any other GUI toolkit) dependency: it exposes
//! [`session::SessionHandle`] as the single entry point a frontend drives, plus the
//! toolkit-agnostic supporting types ([`profile`], [`secrets`], [`known_hosts`], [`events`]).
//! A future non-GTK frontend could be built against this crate unchanged.

pub mod events;
pub mod known_hosts;
pub mod profile;
pub mod secrets;
pub mod session;
