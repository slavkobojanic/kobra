//! The editor protocol of `k preview --editor` (docs/compiler/preview.md,
//! "The editor protocol"): one JSON object per line on standard input
//! (editor to `k`) and standard output (`k` to editor). Offsets are UTF-16
//! code units, as editors count them.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex, mpsc};

use serde_json::Value;

/// A message from the editor.
pub(crate) enum Message {
    /// `didOpen` or `didChange {uri, text}`: the buffer's text, unsaved.
    Change {
        /// The file's `file:` URI.
        uri: String,
        /// Its whole text.
        text: String,
    },
    /// `didClose {uri}`: the file is read from disk again.
    Close {
        /// The file's `file:` URI.
        uri: String,
    },
    /// `select {uri, offset}`: the cursor moved; a preview it is in is
    /// shown.
    Select {
        /// The file's `file:` URI.
        uri: String,
        /// The cursor, in UTF-16 code units.
        offset: usize,
    },
    /// `shutdown`: stop.
    Shutdown,
    /// A line that isn't a message: what is wrong with it.
    Invalid(String),
}

/// Parses one line of the editor's input.
pub(crate) fn parse(line: &str) -> Message {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Message::Invalid(format!("not a JSON message: {line}"));
    };
    let text = |k: &str| v[k].as_str().map(str::to_owned);
    match v["type"].as_str() {
        Some("didOpen" | "didChange") => match (text("uri"), text("text")) {
            (Some(uri), Some(text)) => Message::Change { uri, text },
            _ => Message::Invalid("`didChange` needs `uri` and `text`".to_owned()),
        },
        Some("didClose") => match text("uri") {
            Some(uri) => Message::Close { uri },
            None => Message::Invalid("`didClose` needs `uri`".to_owned()),
        },
        Some("select") => match (
            text("uri"),
            v["offset"].as_u64().and_then(|o| usize::try_from(o).ok()),
        ) {
            (Some(uri), Some(offset)) => Message::Select { uri, offset },
            _ => Message::Invalid("`select` needs `uri` and `offset`".to_owned()),
        },
        Some("shutdown") => Message::Shutdown,
        Some(other) => Message::Invalid(format!("unknown message type `{other}`")),
        None => Message::Invalid("a message needs a `type`".to_owned()),
    }
}

/// Reads the editor's messages from standard input until it ends.
pub(crate) fn read_stdin(tx: &mpsc::Sender<Message>) {
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if tx.send(parse(&line)).is_err() {
            break;
        }
    }
}

/// The editor's end: messages to standard output, one per line.
#[derive(Clone)]
pub(crate) struct Output(Arc<Mutex<std::io::Stdout>>);

impl Output {
    /// Standard output.
    pub(crate) fn stdout() -> Output {
        Output(Arc::new(Mutex::new(std::io::stdout())))
    }

    /// Writes `message` as one line.
    pub(crate) fn send(&self, message: &Value) {
        let mut out = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = writeln!(out, "{message}");
        let _ = out.flush();
    }
}

/// The byte offset of UTF-16 offset `units` in `text` (its end past it).
pub(crate) fn utf16_to_byte(text: &str, units: usize) -> u32 {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen >= units {
            return u32::try_from(i).unwrap_or(u32::MAX);
        }
        seen += c.len_utf16();
    }
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

/// The UTF-16 offset of byte offset `byte` in `text`.
pub(crate) fn byte_to_utf16(text: &str, byte: u32) -> usize {
    let byte = usize::try_from(byte).unwrap_or(usize::MAX).min(text.len());
    text.char_indices()
        .take_while(|(i, _)| *i < byte)
        .map(|(_, c)| c.len_utf16())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_parse() {
        assert!(matches!(
            parse(r#"{"type":"didChange","uri":"file:///a.k","text":"x"}"#),
            Message::Change { .. }
        ));
        assert!(matches!(
            parse(r#"{"type":"select","uri":"file:///a.k","offset":3}"#),
            Message::Select { offset: 3, .. }
        ));
        assert!(matches!(parse(r#"{"type":"shutdown"}"#), Message::Shutdown));
        assert!(matches!(
            parse(r#"{"type":"didChange"}"#),
            Message::Invalid(_)
        ));
        assert!(matches!(parse("nope"), Message::Invalid(_)));
    }

    #[test]
    fn utf16_offsets_convert_both_ways() {
        let text = "é😀x";
        // é is 2 bytes and 1 unit, 😀 4 bytes and 2 units.
        assert_eq!(utf16_to_byte(text, 1), 2);
        assert_eq!(utf16_to_byte(text, 3), 6);
        assert_eq!(utf16_to_byte(text, 9), 7);
        assert_eq!(byte_to_utf16(text, 6), 3);
        assert_eq!(byte_to_utf16(text, 7), 4);
    }
}
