//! The V8 inspector over WebSocket: `k dev --inspect` (server.md §19).
//!
//! A server started with [`crate::ServerOptions::inspect`] runs one worker
//! isolate with V8's inspector on, and serves the DevTools protocol on the
//! inspect address the way Node does, so Chrome DevTools
//! (`chrome://inspect`, "Open dedicated DevTools for Node") and VS Code's
//! `node` attach find it:
//!
//! * `GET /json/version` and `GET /json/list` (also `/json`) describe the
//!   one target, whose `webSocketDebuggerUrl` is `ws://<addr>/ws/k`;
//! * a WebSocket on `/ws/k` is one inspector session: each text frame from
//!   the debugger goes to the isolate's inspector, each message of the
//!   inspector back as a text frame.
//!
//! The generated server modules carry inline source maps to the `.k`
//! files (`k:/_k/src/…`, with the K text embedded in development builds),
//! so breakpoints are set and hit in `.k` lines, and the runtime's modules
//! carry ignore-listed maps so stepping stays in K code.
//!
//! The WebSocket side is the minimum RFC 6455 needs for loopback
//! debuggers: the handshake, unmasking client frames, text and close
//! frames out, fragmented and ping frames in. Each connection gets two
//! threads (reading, writing); the inspector itself runs on the worker's
//! thread, inside its event loop or V8's pause loop.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

use deno_core::futures::StreamExt;
use deno_core::futures::channel::mpsc::{UnboundedSender, unbounded};
use deno_core::{
    InspectorMsg, InspectorSessionChannels, InspectorSessionKind, InspectorSessionProxy,
};

/// The target id in URLs.
const TARGET: &str = "k";

/// Starts serving the inspector of a worker on `listener`; `sessions` is
/// the worker engine's [`crate::Engine::inspector_sessions`]. Returns at
/// once; the server lives on its own thread for the life of the process.
pub(crate) fn serve(
    listener: TcpListener,
    sessions: UnboundedSender<InspectorSessionProxy>,
    title: String,
) {
    let addr = listener.local_addr().ok();
    // The listener closes once the isolate is gone (a `k dev` restart
    // stops the server), so the next server can bind the port again.
    let _ = listener.set_nonblocking(true);
    let _ = std::thread::Builder::new()
        .name("k-inspector".to_owned())
        .spawn(move || {
            while !sessions.is_closed() {
                let stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        continue;
                    }
                    Err(_) => continue,
                };
                let _ = stream.set_nonblocking(false);
                let sessions = sessions.clone();
                let title = title.clone();
                let _ = std::thread::Builder::new()
                    .name("k-inspector-conn".to_owned())
                    .spawn(move || {
                        let _ = connection(stream, addr, &sessions, &title);
                    });
            }
        });
}

/// Binds the inspect address. A port in use is an error for the caller
/// to report.
///
/// # Errors
///
/// The bind failure.
pub fn bind(addr: SocketAddr) -> std::io::Result<TcpListener> {
    // A restarting `k dev` binds while the previous server's inspector
    // is still letting go of the port (it checks every 50 ms).
    let mut tries = 0;
    loop {
        match TcpListener::bind(addr) {
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && tries < 40 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

/// One HTTP request or WebSocket session on the inspect port.
fn connection(
    stream: TcpStream,
    addr: Option<SocketAddr>,
    sessions: &UnboundedSender<InspectorSessionProxy>,
    title: &str,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut key = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((n, v)) = line.split_once(':')
            && n.trim().eq_ignore_ascii_case("sec-websocket-key")
        {
            key = Some(v.trim().to_owned());
        }
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let host = addr.map_or_else(|| "127.0.0.1:9229".to_owned(), |a| a.to_string());
    let mut out = stream;
    match (path, key) {
        (p, Some(key)) if p.starts_with("/ws/") => websocket(out, reader, &key, sessions),
        ("/json/version", _) => respond(
            &mut out,
            &serde_json::json!({
                "Browser": "Kobra",
                "Protocol-Version": "1.3",
            })
            .to_string(),
        ),
        ("/json" | "/json/list", _) => {
            let ws = format!("{host}/ws/{TARGET}");
            respond(
                &mut out,
                &serde_json::json!([{
                    "description": "Kobra server",
                    "devtoolsFrontendUrl": format!("devtools://devtools/bundled/js_app.html?experiments=true&v8only=true&ws={ws}"),
                    "id": TARGET,
                    "title": title,
                    "type": "node",
                    "url": "k:/app/",
                    "webSocketDebuggerUrl": format!("ws://{ws}"),
                }])
                .to_string(),
            )
        }
        _ => out.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n"),
    }
}

/// Writes a JSON response.
fn respond(out: &mut TcpStream, body: &str) -> std::io::Result<()> {
    write!(
        out,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json; charset=utf-8\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
}

/// Upgrades to a WebSocket and bridges it to a new inspector session.
fn websocket(
    mut out: TcpStream,
    mut reader: BufReader<TcpStream>,
    key: &str,
    sessions: &UnboundedSender<InspectorSessionProxy>,
) -> std::io::Result<()> {
    let accept = base64(&sha1(
        format!("{key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11").as_bytes(),
    ));
    write!(
        out,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )?;
    let (to_inspector, from_debugger) = unbounded::<String>();
    let (to_debugger, mut from_inspector) = unbounded::<InspectorMsg>();
    let proxy = InspectorSessionProxy {
        channels: InspectorSessionChannels::Regular {
            tx: to_debugger,
            rx: from_debugger,
        },
        kind: InspectorSessionKind::NonBlocking {
            wait_for_disconnect: false,
        },
    };
    if sessions.unbounded_send(proxy).is_err() {
        return Ok(());
    }
    // Writer: inspector messages to text frames.
    let mut writer = out.try_clone()?;
    let writing = std::thread::Builder::new()
        .name("k-inspector-out".to_owned())
        .spawn(move || {
            while let Some(msg) = deno_core::futures::executor::block_on(from_inspector.next()) {
                if write_frame(&mut writer, 0x1, msg.content.as_bytes()).is_err() {
                    break;
                }
            }
            // The isolate is gone (a restart): close, so the debugger
            // reconnects to the next one.
            let _ = write_frame(&mut writer, 0x8, &[]);
            let _ = writer.shutdown(std::net::Shutdown::Both);
        })?;
    // Reader: text frames to the inspector.
    let mut message = Vec::new();
    while let Some((fin, opcode, payload)) = read_frame(&mut reader)? {
        match opcode {
            0x8 => {
                let _ = write_frame(&mut out, 0x8, &[]);
                break;
            }
            0x9 => write_frame(&mut out, 0xA, &payload)?,
            0x0..=0x2 => {
                message.extend_from_slice(&payload);
                if fin {
                    let text = String::from_utf8_lossy(&message).into_owned();
                    message.clear();
                    if to_inspector.unbounded_send(text).is_err() {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    drop(to_inspector);
    let _ = out.shutdown(std::net::Shutdown::Both);
    let _ = writing.join();
    Ok(())
}

/// Reads one frame: `(fin, opcode, unmasked payload)`; `None` at EOF.
fn read_frame(r: &mut impl Read) -> std::io::Result<Option<(bool, u8, Vec<u8>)>> {
    let mut head = [0u8; 2];
    if r.read_exact(&mut head).is_err() {
        return Ok(None);
    }
    let fin = head[0] & 0x80 != 0;
    let opcode = head[0] & 0x0f;
    let masked = head[1] & 0x80 != 0;
    let mut len = u64::from(head[1] & 0x7f);
    if len == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b)?;
        len = u64::from(u16::from_be_bytes(b));
    } else if len == 127 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b)?;
        len = u64::from_be_bytes(b);
    }
    let mut mask = [0u8; 4];
    if masked {
        r.read_exact(&mut mask)?;
    }
    let mut payload = vec![0u8; usize::try_from(len).unwrap_or(0)];
    r.read_exact(&mut payload)?;
    if masked {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
    }
    Ok(Some((fin, opcode, payload)))
}

/// Writes one unmasked frame (servers don't mask).
fn write_frame(w: &mut impl Write, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    let mut head = vec![0x80 | opcode];
    let len = payload.len();
    if len < 126 {
        head.push(u8::try_from(len).unwrap_or(0));
    } else if let Ok(l) = u16::try_from(len) {
        head.push(126);
        head.extend_from_slice(&l.to_be_bytes());
    } else {
        head.push(127);
        head.extend_from_slice(&(len as u64).to_be_bytes());
    }
    w.write_all(&head)?;
    w.write_all(payload)?;
    w.flush()
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// SHA-1 (RFC 3174), which only the WebSocket handshake needs.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[4 * i],
                block[4 * i + 1],
                block[4 * i + 2],
                block[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_accepts_the_rfc_6455_example() {
        let accept = base64(&sha1(
            b"dGhlIHNhbXBsZSBub25jZQ==258EAFA5-E914-47DA-95CA-C5AB0DC85B11",
        ));
        assert_eq!(accept, "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }
}
