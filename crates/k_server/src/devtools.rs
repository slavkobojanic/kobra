//! `k dev`'s channels inspector at `/_k/dev/channels`
//! (`docs/design/44-channels.md`, "Visibility in development and
//! production").
//!
//! A small server-rendered page, refreshed every two seconds: the open
//! sockets grouped by session, each channel's state, sequence numbers,
//! buffer depth and last messages, the channels waiting for a resume, the
//! presence of each key, the recent open and close lines and the
//! process's metrics. Only a server started with
//! [`crate::ServerOptions::dev_tools`] (`k dev` and `k run`) serves it;
//! `dist/app` never does, so it is never in a `k build`.

use std::fmt::Write as _;
use std::time::Duration;

use crate::process::PresenceKey;
use crate::sockets::{ChannelView, Hub, HubView};

/// The inspector's path.
pub const CHANNELS_PATH: &str = "/_k/dev/channels";

/// `text` escaped for HTML.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// `d` in whole seconds, or minutes and seconds.
fn age(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s} s")
    } else {
        format!("{} min {} s", s / 60, s % 60)
    }
}

/// The rows of `channels`, after a heading row.
fn channel_rows(out: &mut String, channels: &[ChannelView]) {
    out.push_str(
        "<table><tr><th>#</th><th>Method</th><th>User</th><th>State</th><th>Worker</th>\
         <th>Open for</th><th>Sent seq</th><th>Received seq</th><th>Buffer</th>\
         <th>In / out</th><th>Last messages</th></tr>\n",
    );
    for c in channels {
        let ch = c.ch.map_or_else(|| "–".to_owned(), |n| n.to_string());
        let state = match c.resume_left {
            Some(left) => format!("{} ({} left)", c.state, age(left)),
            None => c.state.to_owned(),
        };
        let mut last = String::new();
        for m in &c.last {
            let _ = write!(
                last,
                "<div class=\"m\">{} {} <code>{}</code></div>",
                if m.incoming { "→" } else { "←" },
                m.seq,
                esc(&m.text)
            );
        }
        let _ = writeln!(
            out,
            "<tr><td>{ch} <small>(id {})</small></td><td><code>{}</code></td><td><code>{}</code></td>\
             <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{} msgs, {} B</td>\
             <td>{} / {}</td><td>{last}</td></tr>",
            c.id,
            esc(&c.method),
            esc(c.user.as_deref().unwrap_or("guest")),
            esc(&state),
            c.worker,
            age(c.age),
            c.sent,
            c.received,
            c.buffered,
            c.buffered_bytes,
            c.messages_in,
            c.messages_out,
        );
    }
    out.push_str("</table>\n");
}

/// The inspector page for `hub`'s state, `presence` (each key with its
/// members' codec JSON and lease counts; `None` when presence lives in
/// Postgres) and the hub's metrics.
#[must_use]
pub fn channels_page(hub: &Hub, presence: Option<&[PresenceKey]>) -> String {
    let HubView { sockets, detached } = hub.snapshot();
    let mut out = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta http-equiv=\"refresh\" content=\"2\"><title>Channels · k dev</title><style>\
         body{font:14px/1.4 system-ui,sans-serif;margin:16px;color:#1d1d1f;background:#fff}\
         @media (prefers-color-scheme:dark){body{color:#e8e8ea;background:#141416}th{background:#26262a!important}}\
         table{border-collapse:collapse;margin:8px 0 16px;width:100%}\
         th,td{border:1px solid #8884;padding:4px 6px;text-align:left;vertical-align:top}\
         th{background:#f2f2f4}code{font-size:12px;word-break:break-all}.m{white-space:nowrap}\
         h2{margin-top:24px}pre{font-size:12px;overflow-x:auto}</style></head><body>\n\
         <h1>Channels</h1>\n",
    );
    let channels: usize = sockets.iter().map(|s| s.channels.len()).sum();
    let _ = writeln!(
        out,
        "<p>{} sockets, {} channels, {} waiting for a resume. Refreshes every 2 s.</p>",
        sockets.len(),
        channels,
        detached.len()
    );
    out.push_str("<h2>Sockets by session</h2>\n");
    if sockets.is_empty() {
        out.push_str("<p>No open sockets.</p>\n");
    }
    let mut sessions: Vec<&str> = sockets.iter().map(|s| s.session.as_str()).collect();
    sessions.sort_unstable();
    sessions.dedup();
    for session in sessions {
        let _ = writeln!(out, "<h3>Session <code>{}</code></h3>", esc(session));
        for s in sockets.iter().filter(|s| s.session == session) {
            let _ = writeln!(
                out,
                "<p>Socket {} on worker {}, open for {}</p>",
                s.id,
                s.worker,
                age(s.age)
            );
            channel_rows(&mut out, &s.channels);
        }
    }
    out.push_str("<h2>Waiting for a resume</h2>\n");
    if detached.is_empty() {
        out.push_str("<p>None.</p>\n");
    } else {
        let rows: Vec<ChannelView> = detached.into_iter().map(|(_, c)| c).collect();
        channel_rows(&mut out, &rows);
    }
    out.push_str("<h2>Presence</h2>\n");
    match presence {
        None => out.push_str("<p>Presence is in Postgres (<code>k_presence</code>).</p>\n"),
        Some([]) => out.push_str("<p>No one is present anywhere.</p>\n"),
        Some(keys) => {
            out.push_str("<table><tr><th>Key</th><th>Members (leases)</th></tr>\n");
            for (key, members) in keys {
                let list: Vec<String> = members
                    .iter()
                    .map(|(m, n)| format!("<code>{}</code> ({n})", esc(m)))
                    .collect();
                let _ = writeln!(
                    out,
                    "<tr><td><code>{}</code></td><td>{}</td></tr>",
                    esc(key),
                    list.join("<br>")
                );
            }
            out.push_str("</table>\n");
        }
    }
    out.push_str("<h2>Recent opens and closes</h2>\n<pre>");
    for line in hub.log_lines().iter().rev() {
        out.push_str(&esc(line));
        out.push('\n');
    }
    out.push_str("</pre>\n<h2>Metrics</h2>\n<pre>");
    out.push_str(&esc(&hub.metrics.render()));
    out.push_str("</pre>\n</body></html>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::channels_page;
    use crate::sockets::Hub;

    #[test]
    fn the_page_escapes_and_lists_presence_by_key() {
        let hub = Hub::default();
        let presence = vec![(
            "chat.RoomPresence|\"lobby\"".to_owned(),
            vec![("{\"name\":\"<ana>\"}".to_owned(), 2)],
        )];
        let page = channels_page(&hub, Some(&presence));
        assert!(page.contains("No open sockets."), "{page}");
        assert!(
            page.contains("<code>chat.RoomPresence|&quot;lobby&quot;</code>"),
            "{page}"
        );
        assert!(
            page.contains("<code>{&quot;name&quot;:&quot;&lt;ana&gt;&quot;}</code> (2)"),
            "{page}"
        );
        let pg = channels_page(&hub, None);
        assert!(pg.contains("Presence is in Postgres"), "{pg}");
    }
}
