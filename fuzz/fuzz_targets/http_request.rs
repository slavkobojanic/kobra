//! HTTP request parsing in `k_server`: the input is written as is to a
//! fresh connection of the fuzz app's server (hyper's parser, the front's
//! checks, URL, header, cookie and body handling in the worker). Any answer
//! must be well-formed HTTP/1.1 and never a `500`, and the server must keep
//! serving its page and an rpc afterwards (`horacle::check_raw_request`).
#![no_main]

use k_fuzz::{horacle, server};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Err(e) = horacle::check_raw_request(server(), data) {
        panic!("{e}");
    }
});
