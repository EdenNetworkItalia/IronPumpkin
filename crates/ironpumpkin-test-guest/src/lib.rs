//! The v0.2 Wasm plugin `dispatch-guest` of the cross-channel dispatch tests in
//! `crates/ironpumpkin-mods/tests/wasm_dispatch.rs`. It binds the v0.2 WIT directly, because the
//! guest SDK `pumpkin-plugin-api` binds only the v0.1 WIT. The example `dispatch_peer` builds the
//! same guest as the second plugin `dispatch-peer`.
//!
//! The test builds the release build of both targets itself. It needs the `wasm32-wasip1`
//! standard library:
//!
//! ```text
//! rustup target add wasm32-wasip1
//! cargo test -p ironpumpkin-mods --test wasm_dispatch
//! ```

const PLUGIN_NAME: &str = "dispatch-guest";

include!("guest.rs");
