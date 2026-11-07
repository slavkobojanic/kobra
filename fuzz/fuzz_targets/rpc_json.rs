//! RPC JSON decoding through the real server: the first byte picks an echo
//! rpc of the fuzz app (`crates/k_server/tests/projects/fuzz-wire`), the
//! rest is the request body, posted as `application/json` with
//! `Connect-Protocol-Version: 1`. The answer must be a `200` JSON message
//! that is a fixed point (posting it back answers the same), or a `400
//! invalid_argument` Connect error; never a `500` (`horacle::check_rpc_body`).
#![no_main]

use k_fuzz::{horacle, server};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((index, body)) = data.split_first() else {
        return;
    };
    if let Err(e) = horacle::check_rpc_body(server(), usize::from(*index), body) {
        panic!("{e}");
    }
});
