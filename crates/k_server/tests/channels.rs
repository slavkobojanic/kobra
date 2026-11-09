//! The `k.channel.v1` frame codec (`k_server::channels`) against the golden
//! vectors it shares with the browser runtime's codec
//! (`crates/k_runtime/js/tests/channel-frames.json`, also read by
//! `channels.test.js`): every frame decodes to its value and encodes back to
//! exactly its text or bytes, in JSON and in Protobuf, and every invalid
//! frame is rejected (docs/design/44-channels.md, "The wire").

use k_server::channels::{
    CloseReason, Frame, decode_binary, decode_json, encode_binary, encode_json,
};
use serde_json::Value;

/// The vectors file.
fn vectors() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../k_runtime/js/tests/channel-frames.json");
    let text = std::fs::read_to_string(path).expect("the vectors are readable");
    serde_json::from_str(&text).expect("the vectors are JSON")
}

/// Hex text to bytes.
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Bytes to hex text.
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The frame a vector's decoded form describes, with payloads mapped by
/// `payload` (`None` for an absent or `null` one).
fn frame<P>(v: &Value, payload: &dyn Fn(&Value) -> Option<P>) -> Frame<P> {
    let n = |k: &str| v[k].as_u64().expect(k);
    let ch = || u32::try_from(n("ch")).expect("ch");
    let s = |k: &str| v[k].as_str().expect(k).to_owned();
    let p = |k: &str| payload(&v[k]).expect(k);
    match v["kind"].as_str().expect("kind") {
        "open" => Frame::Open {
            ch: ch(),
            method: s("method"),
            request: p("request"),
        },
        "opened" => Frame::Opened {
            ch: ch(),
            resume: s("resume"),
        },
        "refused" => Frame::Refused {
            ch: ch(),
            error: p("error"),
        },
        "msg" => Frame::Msg {
            ch: ch(),
            seq: n("seq"),
            value: p("value"),
        },
        "ack" => Frame::Ack {
            ch: ch(),
            seq: n("seq"),
            error: payload(&v["error"]),
        },
        "rcvd" => Frame::Rcvd {
            ch: ch(),
            seq: n("seq"),
        },
        "resume" => Frame::Resume {
            ch: ch(),
            token: s("token"),
            received: n("received"),
            sent: n("sent"),
        },
        "close" => Frame::Close {
            ch: ch(),
            reason: CloseReason::from_name(&s("reason")).expect("reason"),
        },
        "session" => Frame::Session {
            user: payload(&v["user"]),
        },
        other => {
            assert_eq!(other, "beat", "an unknown kind");
            Frame::Beat
        }
    }
}

#[test]
fn json_frames_match_the_golden_vectors() {
    let all = vectors();
    for v in all["frames"].as_array().expect("frames") {
        let name = v["name"].as_str().unwrap_or_default();
        let text = v["json"].as_str().expect("json");
        let expected = frame(&v["frame"], &|p: &Value| (!p.is_null()).then(|| p.clone()));
        assert_eq!(decode_json(text).as_ref(), Ok(&expected), "decoding {name}");
        assert_eq!(encode_json(&expected), text, "encoding {name}");
    }
}

#[test]
fn binary_frames_match_the_golden_vectors() {
    let all = vectors();
    for v in all["frames"].as_array().expect("frames") {
        let name = v["name"].as_str().unwrap_or_default();
        let bytes = unhex(v["binary"].as_str().expect("binary"));
        let expected = frame(&v["binaryFrame"], &|p: &Value| p.as_str().map(unhex));
        assert_eq!(
            decode_binary(&bytes).as_ref(),
            Ok(&expected),
            "decoding {name}"
        );
        assert_eq!(
            hex(&encode_binary(&expected)),
            hex(&bytes),
            "encoding {name}"
        );
    }
}

#[test]
fn invalid_frames_are_rejected() {
    let all = vectors();
    for v in all["invalidJson"].as_array().expect("invalidJson") {
        let text = v["json"].as_str().expect("json");
        assert!(decode_json(text).is_err(), "{} decoded", v["name"]);
    }
    for v in all["invalidBinary"].as_array().expect("invalidBinary") {
        let bytes = unhex(v["binary"].as_str().expect("binary"));
        assert!(decode_binary(&bytes).is_err(), "{} decoded", v["name"]);
    }
}

#[test]
fn every_kind_has_a_vector() {
    let all = vectors();
    let kinds: std::collections::BTreeSet<&str> = all["frames"]
        .as_array()
        .expect("frames")
        .iter()
        .filter_map(|v| v["frame"]["kind"].as_str())
        .collect();
    for kind in [
        "open", "opened", "refused", "msg", "ack", "rcvd", "resume", "close", "session", "beat",
    ] {
        assert!(kinds.contains(kind), "no vector of `{kind}`");
    }
}
