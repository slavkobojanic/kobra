//! The JSON-RPC 2.0 transport of the Language Server Protocol: framing
//! (`Content-Length` headers), message classification and error codes.
//!
//! The transport is written for robustness first (`docs/compiler/lsp.md`,
//! "Transport"): a body that isn't JSON, or JSON that isn't a JSON-RPC
//! message, is reported as [`Frame::Malformed`] and the connection goes on,
//! because the framing is intact; a header block without a usable
//! `Content-Length` is skipped up to the next blank line. Only the end of
//! the input or an I/O error ends the connection. Bodies are capped at
//! [`MAX_BODY`] bytes so a hostile length can't exhaust memory.

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

/// The largest body accepted, in bytes. Larger messages are skipped and
/// reported as malformed.
pub const MAX_BODY: usize = 64 << 20;

/// JSON-RPC and LSP error codes.
pub mod codes {
    /// Invalid JSON was received.
    pub const PARSE_ERROR: i64 = -32700;
    /// The JSON is not a valid request object.
    pub const INVALID_REQUEST: i64 = -32600;
    /// The method doesn't exist or is not available.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The parameters are invalid.
    pub const INVALID_PARAMS: i64 = -32602;
    /// An internal error of the server.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// A request arrived before `initialize`.
    pub const SERVER_NOT_INITIALIZED: i64 = -32002;
    /// The request failed, but was syntactically and semantically correct.
    pub const REQUEST_FAILED: i64 = -32803;
    /// The document changed while the request ran; the client may retry.
    pub const CONTENT_MODIFIED: i64 = -32801;
    /// The client cancelled the request.
    pub const REQUEST_CANCELLED: i64 = -32800;
}

/// One message received from the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    /// A request: it has an id and expects exactly one response.
    Request {
        /// The request id (a number or a string).
        id: Value,
        /// The method name.
        method: String,
        /// The parameters; `null` when absent.
        params: Value,
    },
    /// A notification: no id, no response.
    Notification {
        /// The method name.
        method: String,
        /// The parameters; `null` when absent.
        params: Value,
    },
    /// A response to a request the server sent (its content is not used).
    Response {
        /// The id of the server's request.
        id: Value,
    },
}

/// What [`read_frame`] read.
#[derive(Debug)]
pub enum Frame {
    /// A well-formed message.
    Message(Incoming),
    /// A frame whose body is not a JSON-RPC message, or whose header is
    /// unusable. `id` is the request id when one could be read, so the
    /// error response can name it.
    Malformed {
        /// The error code to answer with ([`codes::PARSE_ERROR`] or
        /// [`codes::INVALID_REQUEST`]).
        code: i64,
        /// What was wrong.
        message: String,
        /// The id of the malformed request, if it had a readable one.
        id: Option<Value>,
    },
    /// The input ended.
    Eof,
}

/// Reads the next frame.
///
/// # Errors
///
/// Only I/O errors of `input`; malformed content is a [`Frame::Malformed`].
pub fn read_frame(input: &mut impl BufRead) -> io::Result<Frame> {
    let mut length: Option<usize> = None;
    let mut bad_header: Option<String> = None;
    let mut saw_header = false;
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = input.read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(Frame::Eof);
        }
        let text = String::from_utf8_lossy(&line);
        let text = text.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            if saw_header {
                break;
            }
            // Blank lines between messages are tolerated.
            continue;
        }
        saw_header = true;
        let Some((name, value)) = text.split_once(':') else {
            bad_header.get_or_insert_with(|| format!("a header line without `:`: {text:?}"));
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            match value.trim().parse::<usize>() {
                Ok(n) => length = Some(n),
                Err(_) => {
                    bad_header
                        .get_or_insert_with(|| format!("an invalid Content-Length: {value:?}"));
                }
            }
        }
    }
    let Some(length) = length else {
        return Ok(Frame::Malformed {
            code: codes::PARSE_ERROR,
            message: bad_header.unwrap_or_else(|| "a message without Content-Length".to_owned()),
            id: None,
        });
    };
    if length > MAX_BODY {
        // Skip the body without holding it.
        let mut remaining = length as u64;
        while remaining > 0 {
            let buf = input.fill_buf()?;
            if buf.is_empty() {
                return Ok(Frame::Eof);
            }
            let take = buf
                .len()
                .min(usize::try_from(remaining).unwrap_or(usize::MAX));
            input.consume(take);
            remaining -= take as u64;
        }
        return Ok(Frame::Malformed {
            code: codes::INVALID_REQUEST,
            message: format!("a message of {length} bytes, more than the {MAX_BODY} accepted"),
            id: None,
        });
    }
    let mut body = vec![0u8; length];
    if let Err(e) = input.read_exact(&mut body) {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(Frame::Eof);
        }
        return Err(e);
    }
    Ok(classify(&body))
}

/// Parses and classifies one body.
fn classify(body: &[u8]) -> Frame {
    let value: Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            return Frame::Malformed {
                code: codes::PARSE_ERROR,
                message: format!("the body is not JSON: {e}"),
                id: None,
            };
        }
    };
    let Value::Object(mut object) = value else {
        return Frame::Malformed {
            code: codes::INVALID_REQUEST,
            message: "the body is not a JSON object".to_owned(),
            id: None,
        };
    };
    let id = object
        .remove("id")
        .filter(|id| id.is_number() || id.is_string());
    let params = object.remove("params").unwrap_or(Value::Null);
    match (object.remove("method"), id) {
        (Some(Value::String(method)), Some(id)) => {
            Frame::Message(Incoming::Request { id, method, params })
        }
        (Some(Value::String(method)), None) => {
            Frame::Message(Incoming::Notification { method, params })
        }
        (None, Some(id)) if object.contains_key("result") || object.contains_key("error") => {
            Frame::Message(Incoming::Response { id })
        }
        (_, id) => Frame::Malformed {
            code: codes::INVALID_REQUEST,
            message: "not a request, notification or response".to_owned(),
            id,
        },
    }
}

/// Writes one message with its `Content-Length` header and flushes.
///
/// # Errors
///
/// I/O errors of `output`.
pub fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

/// A successful response.
#[must_use]
pub fn response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// An error response; `id` is `null` when the request's id is unknown.
#[must_use]
pub fn error_response(id: Option<&Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id.cloned().unwrap_or(Value::Null),
        "error": { "code": code, "message": message },
    })
}

/// A notification from the server.
#[must_use]
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// A request from the server to the client.
#[must_use]
pub fn request(id: &Value, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(body: &str) -> String {
        format!("Content-Length: {}\r\n\r\n{body}", body.len())
    }

    fn read_all(input: &str) -> Vec<Frame> {
        let mut reader = io::BufReader::new(input.as_bytes());
        let mut out = Vec::new();
        loop {
            let f = read_frame(&mut reader).unwrap();
            if matches!(f, Frame::Eof) {
                break;
            }
            out.push(f);
        }
        out
    }

    #[test]
    fn messages_are_classified() {
        let input = [
            frame(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
            frame(r#"{"jsonrpc":"2.0","method":"initialized"}"#),
            frame(r#"{"jsonrpc":"2.0","id":"a","result":null}"#),
        ]
        .concat();
        let frames = read_all(&input);
        assert!(matches!(
            &frames[0],
            Frame::Message(Incoming::Request { id, method, .. }) if *id == json!(1) && method == "initialize"
        ));
        assert!(matches!(
            &frames[1],
            Frame::Message(Incoming::Notification { method, params }) if method == "initialized" && params.is_null()
        ));
        assert!(
            matches!(&frames[2], Frame::Message(Incoming::Response { id }) if *id == json!("a"))
        );
    }

    #[test]
    fn malformed_bodies_keep_the_connection() {
        let input = [
            frame("{not json"),
            frame("[1, 2]"),
            frame(r#"{"id": 7}"#),
            "Content-Length: nope\r\n\r\n".to_owned(),
            "X-Other: 1\r\n\r\n".to_owned(),
            frame(r#"{"method":"exit"}"#),
        ]
        .concat();
        let frames = read_all(&input);
        assert_eq!(frames.len(), 6, "{frames:?}");
        assert!(matches!(
            &frames[0],
            Frame::Malformed {
                code: codes::PARSE_ERROR,
                ..
            }
        ));
        assert!(matches!(
            &frames[1],
            Frame::Malformed {
                code: codes::INVALID_REQUEST,
                ..
            }
        ));
        assert!(matches!(&frames[2], Frame::Malformed { id: Some(id), .. } if *id == json!(7)));
        assert!(matches!(&frames[3], Frame::Malformed { .. }));
        assert!(matches!(&frames[4], Frame::Malformed { .. }));
        assert!(matches!(
            &frames[5],
            Frame::Message(Incoming::Notification { .. })
        ));
    }

    #[test]
    fn truncated_input_is_the_end() {
        assert!(read_all("Content-Length: 50\r\n\r\n{\"a\"").is_empty());
        assert!(read_all("Content-Len").is_empty());
    }

    #[test]
    fn oversized_bodies_are_skipped() {
        let input = format!("Content-Length: {}\r\n\r\n", MAX_BODY + 1);
        let frames = read_all(&input);
        // The body is missing, so the input ends while it is skipped.
        assert!(frames.is_empty());
    }

    #[test]
    fn written_messages_read_back() {
        let mut out = Vec::new();
        write_message(&mut out, &notification("x", json!({"é": 1}))).unwrap();
        let text = String::from_utf8(out).unwrap();
        let frames = read_all(&text);
        assert!(
            matches!(&frames[0], Frame::Message(Incoming::Notification { method, .. }) if method == "x")
        );
    }
}
