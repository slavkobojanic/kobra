//! The frames of `k.channel.v1`, the channels' WebSocket protocol
//! (`docs/design/44-channels.md`, "The wire"), as pure encode and decode.
//!
//! Nothing serves a socket yet: this is the codec the Rust front and the
//! browser runtime (`runtime/channels.js`) share, tested against the same
//! golden vectors (`crates/k_runtime/js/tests/channel-frames.json`).
//!
//! A frame is one of ten kinds. Its payloads (an open's `request`, a
//! message's `value`, a refusal's `error`, the session mirror's `user`)
//! are messages of the app's protocols, already encoded: JSON values in
//! development, where frames are JSON text, and Protobuf bytes in
//! production, where frames are binary. The codec doesn't look inside
//! them, so a frame is generic over its payload `P`.
//!
//! **JSON text** (development): one object with one key, the kind, as in
//! the design: `{"msg":{"ch":1,"seq":1,"value":{…}}}`. Numbers are JSON
//! numbers (at most 2^53 − 1); `ack.error` is left out when the message was
//! accepted, and `session.user` is `null` for a guest.
//!
//! **Protobuf** (production): the message `k.channel.v1.Frame`, a `oneof`
//! of the kinds, with payloads as `bytes`:
//!
//! ```text
//! message Frame {
//!   oneof frame {
//!     Open open = 1; Opened opened = 2; Refused refused = 3; Msg msg = 4; Ack ack = 5;
//!     Rcvd rcvd = 6; Resume resume = 7; Close close = 8; Session session = 9; Beat beat = 10;
//!   }
//! }
//! message Open    { uint32 ch = 1; string method = 2; bytes request = 3; }
//! message Opened  { uint32 ch = 1; string resume = 2; }
//! message Refused { uint32 ch = 1; bytes error = 2; }
//! message Msg     { uint32 ch = 1; uint64 seq = 2; bytes value = 3; }
//! message Ack     { uint32 ch = 1; uint64 seq = 2; optional bytes error = 3; }
//! message Rcvd    { uint32 ch = 1; uint64 seq = 2; }
//! message Resume  { uint32 ch = 1; string token = 2; uint64 received = 3; uint64 sent = 4; }
//! message Close   { uint32 ch = 1; Reason reason = 2; }
//! message Session { optional bytes user = 1; }
//! message Beat    {}
//! enum Reason { REASON_UNSPECIFIED = 0; BY_CLIENT = 1; BY_SERVER = 2; SESSION_ENDED = 3; REFUSED = 4; }
//! ```
//!
//! Encoding is canonical proto3: fields in number order, scalar zeros and
//! empty strings and bytes left out, except the two `optional` fields,
//! which are written whenever present. Decoding skips unknown fields and
//! rejects truncated or mistyped ones.

use std::fmt;

use serde_json::Value;

/// The path the browser opens the channels' WebSocket at.
pub const PATH: &str = "/_k/channels";

/// The WebSocket subprotocol of channel sockets.
pub const SUBPROTOCOL: &str = "k.channel.v1";

/// The largest sequence number or count a JSON frame carries exactly.
const MAX_SAFE: u64 = (1 << 53) - 1;

/// Why a channel closed for good, as `k.rpc.ChannelClose` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseReason {
    /// `close()`, or the owner that opened it was cancelled.
    ByClient,
    /// The handler's sending stream ended.
    ByServer,
    /// The session ended.
    SessionEnded,
    /// A restart was refused.
    Refused,
}

impl CloseReason {
    /// Every reason, in Protobuf number order (from 1).
    const ALL: [CloseReason; 4] = [
        CloseReason::ByClient,
        CloseReason::ByServer,
        CloseReason::SessionEnded,
        CloseReason::Refused,
    ];

    /// The case name, as JSON frames write it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            CloseReason::ByClient => "byClient",
            CloseReason::ByServer => "byServer",
            CloseReason::SessionEnded => "sessionEnded",
            CloseReason::Refused => "refused",
        }
    }

    /// The reason named `name`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<CloseReason> {
        CloseReason::ALL.into_iter().find(|r| r.name() == name)
    }

    /// The Protobuf enum number.
    fn number(self) -> u64 {
        match self {
            CloseReason::ByClient => 1,
            CloseReason::ByServer => 2,
            CloseReason::SessionEnded => 3,
            CloseReason::Refused => 4,
        }
    }

    /// The reason numbered `n`.
    fn from_number(n: u64) -> Option<CloseReason> {
        CloseReason::ALL.into_iter().find(|r| r.number() == n)
    }
}

/// One frame, with payloads of type `P` (`serde_json::Value` for JSON
/// frames, `Vec<u8>` for binary ones).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame<P> {
    /// Client: opens channel `ch` by calling `method` with `request`.
    Open {
        /// The channel's number, chosen by the client.
        ch: u32,
        /// The rpc's path, `/huddle.chat.ChatProtocol/Room`.
        method: String,
        /// The rpc's parameters.
        request: P,
    },
    /// Server: the channel is open; `resume` is its resume token.
    Opened {
        /// The channel.
        ch: u32,
        /// The token a reconnect presents.
        resume: String,
    },
    /// Server: the open was refused with the rpc's error.
    Refused {
        /// The channel.
        ch: u32,
        /// The rpc's error.
        error: P,
    },
    /// Either side: message number `seq` of its direction.
    Msg {
        /// The channel.
        ch: u32,
        /// The message's sequence number, from 1.
        seq: u64,
        /// The message.
        value: P,
    },
    /// Server: client message `seq` was handled, or refused with `error`.
    Ack {
        /// The channel.
        ch: u32,
        /// The message acknowledged.
        seq: u64,
        /// The refusal `E`, when the message was refused.
        error: Option<P>,
    },
    /// Client: it has received the server's messages up to `seq`.
    Rcvd {
        /// The channel.
        ch: u32,
        /// The highest sequence number received.
        seq: u64,
    },
    /// Client: reattaches channel `ch` after a drop.
    Resume {
        /// The channel.
        ch: u32,
        /// The resume token from `opened`.
        token: String,
        /// The highest server message received.
        received: u64,
        /// The highest client message sent.
        sent: u64,
    },
    /// Either side: the channel closed for good.
    Close {
        /// The channel.
        ch: u32,
        /// Why.
        reason: CloseReason,
    },
    /// Server: the session mirror after the session changed; `None` for a
    /// guest.
    Session {
        /// The new mirror.
        user: Option<P>,
    },
    /// Either side: a heartbeat.
    Beat,
}

impl<P> Frame<P> {
    /// The kind, as JSON frames name it (`msg`).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Frame::Open { .. } => "open",
            Frame::Opened { .. } => "opened",
            Frame::Refused { .. } => "refused",
            Frame::Msg { .. } => "msg",
            Frame::Ack { .. } => "ack",
            Frame::Rcvd { .. } => "rcvd",
            Frame::Resume { .. } => "resume",
            Frame::Close { .. } => "close",
            Frame::Session { .. } => "session",
            Frame::Beat => "beat",
        }
    }
}

/// Why a frame couldn't be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameError(pub String);

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid channel frame: {}", self.0)
    }
}

impl std::error::Error for FrameError {}

/// A [`FrameError`] with `message`.
fn bad<T>(message: impl Into<String>) -> Result<T, FrameError> {
    Err(FrameError(message.into()))
}

// ------------------------------------------------------------------ JSON

/// Encodes a JSON text frame, with keys in the order of the design.
#[must_use]
pub fn encode_json(frame: &Frame<Value>) -> String {
    let s = |text: &str| Value::String(text.to_owned()).to_string();
    let body = match frame {
        Frame::Open {
            ch,
            method,
            request,
        } => format!(r#""ch":{ch},"method":{},"request":{request}"#, s(method)),
        Frame::Opened { ch, resume } => format!(r#""ch":{ch},"resume":{}"#, s(resume)),
        Frame::Refused { ch, error } => format!(r#""ch":{ch},"error":{error}"#),
        Frame::Msg { ch, seq, value } => format!(r#""ch":{ch},"seq":{seq},"value":{value}"#),
        Frame::Ack { ch, seq, error } => match error {
            Some(e) => format!(r#""ch":{ch},"seq":{seq},"error":{e}"#),
            None => format!(r#""ch":{ch},"seq":{seq}"#),
        },
        Frame::Rcvd { ch, seq } => format!(r#""ch":{ch},"seq":{seq}"#),
        Frame::Resume {
            ch,
            token,
            received,
            sent,
        } => format!(
            r#""ch":{ch},"token":{},"received":{received},"sent":{sent}"#,
            s(token)
        ),
        Frame::Close { ch, reason } => format!(r#""ch":{ch},"reason":"{}""#, reason.name()),
        Frame::Session { user } => match user {
            Some(u) => format!(r#""user":{u}"#),
            None => r#""user":null"#.to_owned(),
        },
        Frame::Beat => String::new(),
    };
    format!(r#"{{"{}":{{{body}}}}}"#, frame.kind())
}

/// Decodes a JSON text frame.
///
/// # Errors
///
/// The text isn't one JSON object with one known kind whose fields have
/// the types above; unknown fields inside a kind are ignored.
pub fn decode_json(text: &str) -> Result<Frame<Value>, FrameError> {
    let value: Value = serde_json::from_str(text).map_err(|e| FrameError(e.to_string()))?;
    let Value::Object(outer) = value else {
        return bad("a frame is a JSON object");
    };
    if outer.len() != 1 {
        return bad("a frame has exactly one kind");
    }
    let Some((kind, body)) = outer.into_iter().next() else {
        return bad("a frame has exactly one kind");
    };
    let Value::Object(mut f) = body else {
        return bad(format!("`{kind}` is a JSON object"));
    };
    let mut take = |key: &str| f.remove(key);
    let uint = |v: Option<Value>, key: &str| -> Result<u64, FrameError> {
        match v.as_ref().and_then(Value::as_u64) {
            Some(n) if n <= MAX_SAFE => Ok(n),
            _ => bad(format!(
                "`{kind}.{key}` is a whole number from 0 to 2^53 - 1"
            )),
        }
    };
    let ch = |v: Option<Value>| -> Result<u32, FrameError> {
        v.as_ref()
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n > 0)
            .map_or_else(
                || bad(format!("`{kind}.ch` is a channel number from 1")),
                Ok,
            )
    };
    let string = |v: Option<Value>, key: &str| -> Result<String, FrameError> {
        match v {
            Some(Value::String(s)) => Ok(s),
            _ => bad(format!("`{kind}.{key}` is a string")),
        }
    };
    let payload = |v: Option<Value>, key: &str| -> Result<Value, FrameError> {
        v.map_or_else(|| bad(format!("`{kind}.{key}` is missing")), Ok)
    };
    Ok(match kind.as_str() {
        "open" => Frame::Open {
            ch: ch(take("ch"))?,
            method: string(take("method"), "method")?,
            request: payload(take("request"), "request")?,
        },
        "opened" => Frame::Opened {
            ch: ch(take("ch"))?,
            resume: string(take("resume"), "resume")?,
        },
        "refused" => Frame::Refused {
            ch: ch(take("ch"))?,
            error: payload(take("error"), "error")?,
        },
        "msg" => Frame::Msg {
            ch: ch(take("ch"))?,
            seq: uint(take("seq"), "seq")?,
            value: payload(take("value"), "value")?,
        },
        "ack" => Frame::Ack {
            ch: ch(take("ch"))?,
            seq: uint(take("seq"), "seq")?,
            error: take("error").filter(|e| !e.is_null()),
        },
        "rcvd" => Frame::Rcvd {
            ch: ch(take("ch"))?,
            seq: uint(take("seq"), "seq")?,
        },
        "resume" => Frame::Resume {
            ch: ch(take("ch"))?,
            token: string(take("token"), "token")?,
            received: uint(take("received"), "received")?,
            sent: uint(take("sent"), "sent")?,
        },
        "close" => {
            let ch = ch(take("ch"))?;
            let name = string(take("reason"), "reason")?;
            let Some(reason) = CloseReason::from_name(&name) else {
                return bad(format!("`{name}` isn't a close reason"));
            };
            Frame::Close { ch, reason }
        }
        "session" => Frame::Session {
            user: take("user").filter(|u| !u.is_null()),
        },
        "beat" => Frame::Beat,
        other => return bad(format!("`{other}` isn't a kind of frame")),
    })
}

// -------------------------------------------------------------- Protobuf

/// Appends `v` as a base-128 varint.
fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(u8::try_from(v & 0x7f).unwrap_or(0) | 0x80);
        v >>= 7;
    }
    out.push(u8::try_from(v).unwrap_or(0));
}

/// Appends field `n` as a varint, unless it is zero.
fn uint_field(out: &mut Vec<u8>, n: u64, v: u64) {
    if v != 0 {
        varint(out, n << 3);
        varint(out, v);
    }
}

/// Appends field `n` as length-delimited bytes; empty bytes are left out
/// unless `always`.
fn bytes_field(out: &mut Vec<u8>, n: u64, b: &[u8], always: bool) {
    if always || !b.is_empty() {
        varint(out, (n << 3) | 2);
        varint(out, b.len() as u64);
        out.extend_from_slice(b);
    }
}

/// Encodes a binary frame (`k.channel.v1.Frame`).
#[must_use]
pub fn encode_binary(frame: &Frame<Vec<u8>>) -> Vec<u8> {
    let mut body = Vec::new();
    let b = &mut body;
    let number = match frame {
        Frame::Open {
            ch,
            method,
            request,
        } => {
            uint_field(b, 1, u64::from(*ch));
            bytes_field(b, 2, method.as_bytes(), false);
            bytes_field(b, 3, request, false);
            1
        }
        Frame::Opened { ch, resume } => {
            uint_field(b, 1, u64::from(*ch));
            bytes_field(b, 2, resume.as_bytes(), false);
            2
        }
        Frame::Refused { ch, error } => {
            uint_field(b, 1, u64::from(*ch));
            bytes_field(b, 2, error, false);
            3
        }
        Frame::Msg { ch, seq, value } => {
            uint_field(b, 1, u64::from(*ch));
            uint_field(b, 2, *seq);
            bytes_field(b, 3, value, false);
            4
        }
        Frame::Ack { ch, seq, error } => {
            uint_field(b, 1, u64::from(*ch));
            uint_field(b, 2, *seq);
            if let Some(e) = error {
                bytes_field(b, 3, e, true);
            }
            5
        }
        Frame::Rcvd { ch, seq } => {
            uint_field(b, 1, u64::from(*ch));
            uint_field(b, 2, *seq);
            6
        }
        Frame::Resume {
            ch,
            token,
            received,
            sent,
        } => {
            uint_field(b, 1, u64::from(*ch));
            bytes_field(b, 2, token.as_bytes(), false);
            uint_field(b, 3, *received);
            uint_field(b, 4, *sent);
            7
        }
        Frame::Close { ch, reason } => {
            uint_field(b, 1, u64::from(*ch));
            uint_field(b, 2, reason.number());
            8
        }
        Frame::Session { user } => {
            if let Some(u) = user {
                bytes_field(b, 1, u, true);
            }
            9
        }
        Frame::Beat => 10,
    };
    let mut out = Vec::with_capacity(body.len() + 3);
    bytes_field(&mut out, number, &body, true);
    out
}

/// A field read from a Protobuf message.
enum Field<'a> {
    /// Wire type 0.
    Varint(u64),
    /// Wire type 2.
    Bytes(&'a [u8]),
    /// Wire types 1 and 5, which no frame field uses.
    Other,
}

/// Reads the fields of message `buf`, in order.
fn fields(mut buf: &[u8]) -> Result<Vec<(u64, Field<'_>)>, FrameError> {
    /// Reads a varint from the front of `buf`.
    fn read_varint(buf: &mut &[u8]) -> Result<u64, FrameError> {
        let mut v: u64 = 0;
        for i in 0..10 {
            let Some((&byte, rest)) = buf.split_first() else {
                return bad("a varint is cut short");
            };
            *buf = rest;
            v |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        bad("a varint is longer than ten bytes")
    }
    let mut out = Vec::new();
    while !buf.is_empty() {
        let key = read_varint(&mut buf)?;
        let n = key >> 3;
        if n == 0 {
            return bad("field number 0");
        }
        let field = match key & 7 {
            0 => Field::Varint(read_varint(&mut buf)?),
            2 => {
                let len = usize::try_from(read_varint(&mut buf)?).unwrap_or(usize::MAX);
                if len > buf.len() {
                    return bad("a field is longer than the frame");
                }
                let (b, rest) = buf.split_at(len);
                buf = rest;
                Field::Bytes(b)
            }
            1 if buf.len() >= 8 => {
                buf = &buf[8..];
                Field::Other
            }
            5 if buf.len() >= 4 => {
                buf = &buf[4..];
                Field::Other
            }
            1 | 5 => return bad("a fixed-width field is cut short"),
            t => return bad(format!("wire type {t} isn't supported")),
        };
        out.push((n, field));
    }
    Ok(out)
}

/// The fields of one kind's message, by number; the last occurrence wins,
/// as in Protobuf.
struct Message<'a> {
    /// The kind, for errors.
    kind: &'static str,
    /// The fields read.
    fields: Vec<(u64, Field<'a>)>,
}

impl<'a> Message<'a> {
    /// The last field numbered `n`.
    fn get(&self, n: u64) -> Option<&Field<'a>> {
        self.fields
            .iter()
            .rev()
            .find(|(m, _)| *m == n)
            .map(|(_, f)| f)
    }

    /// Varint field `n`, zero when absent.
    fn uint(&self, n: u64) -> Result<u64, FrameError> {
        match self.get(n) {
            None => Ok(0),
            Some(Field::Varint(v)) => Ok(*v),
            Some(_) => bad(format!("field {n} of `{}` isn't a varint", self.kind)),
        }
    }

    /// The channel number, field 1, from 1.
    fn ch(&self) -> Result<u32, FrameError> {
        match u32::try_from(self.uint(1)?) {
            Ok(ch) if ch > 0 => Ok(ch),
            _ => bad(format!("`{}` needs a channel number from 1", self.kind)),
        }
    }

    /// Bytes field `n`, `None` when absent.
    fn bytes(&self, n: u64) -> Result<Option<Vec<u8>>, FrameError> {
        match self.get(n) {
            None => Ok(None),
            Some(Field::Bytes(b)) => Ok(Some(b.to_vec())),
            Some(_) => bad(format!(
                "field {n} of `{}` isn't length-delimited",
                self.kind
            )),
        }
    }

    /// String field `n`, empty when absent.
    fn string(&self, n: u64) -> Result<String, FrameError> {
        String::from_utf8(self.bytes(n)?.unwrap_or_default())
            .map_err(|_| FrameError(format!("field {n} of `{}` isn't UTF-8", self.kind)))
    }
}

/// Decodes a binary frame.
///
/// # Errors
///
/// The bytes aren't a `k.channel.v1.Frame` with one known kind, or a field
/// has the wrong wire type.
pub fn decode_binary(bytes: &[u8]) -> Result<Frame<Vec<u8>>, FrameError> {
    const KINDS: [&str; 10] = [
        "open", "opened", "refused", "msg", "ack", "rcvd", "resume", "close", "session", "beat",
    ];
    let Some((n, body)) = fields(bytes)?.into_iter().rev().find_map(|(n, f)| match f {
        Field::Bytes(b) if (1..=10).contains(&n) => Some((n, b)),
        _ => None,
    }) else {
        return bad("no known kind of frame");
    };
    let kind = KINDS[usize::try_from(n - 1).unwrap_or(0)];
    let m = Message {
        kind,
        fields: fields(body)?,
    };
    Ok(match n {
        1 => Frame::Open {
            ch: m.ch()?,
            method: m.string(2)?,
            request: m.bytes(3)?.unwrap_or_default(),
        },
        2 => Frame::Opened {
            ch: m.ch()?,
            resume: m.string(2)?,
        },
        3 => Frame::Refused {
            ch: m.ch()?,
            error: m.bytes(2)?.unwrap_or_default(),
        },
        4 => Frame::Msg {
            ch: m.ch()?,
            seq: m.uint(2)?,
            value: m.bytes(3)?.unwrap_or_default(),
        },
        5 => Frame::Ack {
            ch: m.ch()?,
            seq: m.uint(2)?,
            error: m.bytes(3)?,
        },
        6 => Frame::Rcvd {
            ch: m.ch()?,
            seq: m.uint(2)?,
        },
        7 => Frame::Resume {
            ch: m.ch()?,
            token: m.string(2)?,
            received: m.uint(3)?,
            sent: m.uint(4)?,
        },
        8 => {
            let ch = m.ch()?;
            let r = m.uint(2)?;
            let Some(reason) = CloseReason::from_number(r) else {
                return bad(format!("{r} isn't a close reason"));
            };
            Frame::Close { ch, reason }
        }
        9 => Frame::Session { user: m.bytes(1)? },
        _ => Frame::Beat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varints_and_kinds_round_trip() {
        for seq in [0, 1, 127, 128, 300, MAX_SAFE, u64::MAX] {
            let f = Frame::Msg {
                ch: 7,
                seq,
                value: vec![1, 2, 3],
            };
            assert_eq!(decode_binary(&encode_binary(&f)), Ok(f));
        }
        let f: Frame<Vec<u8>> = Frame::Ack {
            ch: 1,
            seq: 2,
            error: Some(Vec::new()),
        };
        assert_eq!(decode_binary(&encode_binary(&f)), Ok(f));
    }

    #[test]
    fn an_unknown_field_is_skipped() {
        // msg { ch: 1, seq: 2, 15: fixed32 } and a frame-level field 20.
        let bytes = [
            0x22, 0x09, 0x08, 0x01, 0x10, 0x02, 0x7d, 1, 2, 3, 4, 0xa0, 0x01, 0x05,
        ];
        assert_eq!(
            decode_binary(&bytes),
            Ok(Frame::Msg {
                ch: 1,
                seq: 2,
                value: Vec::new()
            })
        );
    }
}
