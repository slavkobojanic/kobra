//! The process's `k.observe` metrics and trace spans
//! (`docs/design/44-channels.md`, "Visibility in development and
//! production").
//!
//! A small registry the Rust parts of the server record into: counters,
//! gauges and histograms, each a name with labels, and the most recent
//! trace spans. Channels are its first users (the `k_channel_*` metrics
//! and the `k.channel.open` / `k.channel.message` spans, recorded by
//! [`crate::sockets`]), with the delivery lag of broadcasts between
//! replicas (`k_broadcast_lag_seconds`, [`crate::replicas`]).
//!
//! [`Metrics::render`] writes the Prometheus text exposition format, which
//! `k dev`'s inspector shows. Exporting to an OpenTelemetry collector is
//! the book's Planned `observe-metrics`; until then, a deployed app keeps
//! its metrics in memory and tests read them through the accessors.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

/// The upper bounds (seconds) of the latency histograms' buckets; an
/// implicit `+Inf` bucket follows.
pub const LATENCY_BUCKETS: [f64; 12] = [
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// How many spans the registry keeps, newest last.
pub const SPANS_KEPT: usize = 256;

/// A metric's name and its labels, sorted by label name.
type Key = (String, Vec<(String, String)>);

/// One histogram: a count per bucket of [`LATENCY_BUCKETS`] (not
/// cumulative) plus the `+Inf` bucket, the number of observations and
/// their sum.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Histogram {
    /// Observations per bucket, the last one `+Inf`.
    pub buckets: Vec<u64>,
    /// How many observations.
    pub count: u64,
    /// Their sum, in seconds.
    pub sum: f64,
}

/// One finished trace span.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// `k.channel.open` or `k.channel.message`.
    pub name: &'static str,
    /// Its attributes, in the order recorded.
    pub attributes: Vec<(String, String)>,
    /// When it started.
    pub start: SystemTime,
    /// How long it took.
    pub duration: Duration,
}

impl Span {
    /// The value of attribute `name`, if it has one.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The registry's state.
#[derive(Debug, Default)]
struct Registry {
    /// Counters by name and labels.
    counters: BTreeMap<Key, u64>,
    /// Gauges by name and labels.
    gauges: BTreeMap<Key, i64>,
    /// Histograms by name and labels.
    histograms: BTreeMap<Key, Histogram>,
    /// The most recent spans, oldest first.
    spans: VecDeque<Span>,
}

/// A process's metrics and recent spans. Every method takes `&self`.
#[derive(Debug, Default)]
pub struct Metrics {
    /// The state.
    registry: Mutex<Registry>,
}

/// `name` with `labels` as a registry key.
fn key(name: &str, labels: &[(&str, &str)]) -> Key {
    let mut l: Vec<(String, String)> = labels
        .iter()
        .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
        .collect();
    l.sort();
    (name.to_owned(), l)
}

impl Metrics {
    /// Locks the state, recovering it if a panicking thread poisoned it.
    fn registry(&self) -> MutexGuard<'_, Registry> {
        self.registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Adds `by` to counter `name` with `labels`.
    pub fn count(&self, name: &str, labels: &[(&str, &str)], by: u64) {
        *self
            .registry()
            .counters
            .entry(key(name, labels))
            .or_insert(0) += by;
    }

    /// Moves gauge `name` with `labels` by `delta`.
    pub fn gauge(&self, name: &str, labels: &[(&str, &str)], delta: i64) {
        *self.registry().gauges.entry(key(name, labels)).or_insert(0) += delta;
    }

    /// Records `seconds` in histogram `name` with `labels`.
    pub fn observe(&self, name: &str, labels: &[(&str, &str)], seconds: f64) {
        let mut r = self.registry();
        let h = r.histograms.entry(key(name, labels)).or_default();
        if h.buckets.is_empty() {
            h.buckets = vec![0; LATENCY_BUCKETS.len() + 1];
        }
        let i = LATENCY_BUCKETS
            .iter()
            .position(|&b| seconds <= b)
            .unwrap_or(LATENCY_BUCKETS.len());
        h.buckets[i] += 1;
        h.count += 1;
        h.sum += seconds;
    }

    /// Records a finished span.
    pub fn span(
        &self,
        name: &'static str,
        attributes: &[(&str, &str)],
        start: SystemTime,
        duration: Duration,
    ) {
        let mut r = self.registry();
        r.spans.push_back(Span {
            name,
            attributes: attributes
                .iter()
                .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
                .collect(),
            start,
            duration,
        });
        while r.spans.len() > SPANS_KEPT {
            r.spans.pop_front();
        }
    }

    /// Counter `name` with exactly `labels` (0 if never counted).
    #[must_use]
    pub fn counter(&self, name: &str, labels: &[(&str, &str)]) -> u64 {
        self.registry()
            .counters
            .get(&key(name, labels))
            .copied()
            .unwrap_or(0)
    }

    /// Gauge `name` with exactly `labels` (0 if never moved).
    #[must_use]
    pub fn gauge_value(&self, name: &str, labels: &[(&str, &str)]) -> i64 {
        self.registry()
            .gauges
            .get(&key(name, labels))
            .copied()
            .unwrap_or(0)
    }

    /// Histogram `name` with exactly `labels`, if anything was observed.
    #[must_use]
    pub fn histogram(&self, name: &str, labels: &[(&str, &str)]) -> Option<Histogram> {
        self.registry().histograms.get(&key(name, labels)).cloned()
    }

    /// The most recent spans, oldest first.
    #[must_use]
    pub fn spans(&self) -> Vec<Span> {
        self.registry().spans.iter().cloned().collect()
    }

    /// Every metric in the Prometheus text exposition format (version
    /// 0.0.4), by name: a `# TYPE` line, then one sample per label set;
    /// a histogram as cumulative `_bucket` samples with `le`, `_sum` and
    /// `_count`.
    #[must_use]
    pub fn render(&self) -> String {
        let r = self.registry();
        let mut out = String::new();
        let typed = |out: &mut String, name: &str, kind: &str, last: &mut String| {
            if last != name {
                let _ = writeln!(out, "# TYPE {name} {kind}");
                name.clone_into(last);
            }
        };
        let mut last = String::new();
        for ((name, labels), v) in &r.counters {
            typed(&mut out, name, "counter", &mut last);
            let _ = writeln!(out, "{name}{} {v}", labels_text(labels, None));
        }
        for ((name, labels), v) in &r.gauges {
            typed(&mut out, name, "gauge", &mut last);
            let _ = writeln!(out, "{name}{} {v}", labels_text(labels, None));
        }
        for ((name, labels), h) in &r.histograms {
            typed(&mut out, name, "histogram", &mut last);
            let mut total = 0;
            for (i, n) in h.buckets.iter().enumerate() {
                total += n;
                let le = LATENCY_BUCKETS
                    .get(i)
                    .map_or_else(|| "+Inf".to_owned(), ToString::to_string);
                let _ = writeln!(
                    out,
                    "{name}_bucket{} {total}",
                    labels_text(labels, Some(&le))
                );
            }
            let _ = writeln!(out, "{name}_sum{} {}", labels_text(labels, None), h.sum);
            let _ = writeln!(out, "{name}_count{} {}", labels_text(labels, None), h.count);
        }
        out
    }
}

/// `{a="1",b="2"}` (with `le` last when given), or empty without labels.
fn labels_text(labels: &[(String, String)], le: Option<&str>) -> String {
    let mut parts: Vec<String> = labels
        .iter()
        .map(|(n, v)| format!("{n}=\"{}\"", escape(v)))
        .collect();
    if let Some(le) = le {
        parts.push(format!("le=\"{le}\""));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(","))
    }
}

/// A label value escaped for the text format.
fn escape(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::Metrics;
    use std::time::{Duration, SystemTime};

    #[test]
    fn counters_gauges_and_histograms_render_in_the_text_format() {
        let m = Metrics::default();
        m.count("k_x_total", &[("method", "/a.B/C"), ("direction", "in")], 2);
        m.count("k_x_total", &[("direction", "in"), ("method", "/a.B/C")], 1);
        m.gauge("k_open", &[], 3);
        m.gauge("k_open", &[], -1);
        m.observe("k_lat_seconds", &[("method", "m")], 0.003);
        m.observe("k_lat_seconds", &[("method", "m")], 20.0);
        assert_eq!(
            m.counter("k_x_total", &[("direction", "in"), ("method", "/a.B/C")]),
            3
        );
        assert_eq!(m.gauge_value("k_open", &[]), 2);
        let h = m.histogram("k_lat_seconds", &[("method", "m")]).unwrap();
        assert_eq!((h.count, h.buckets[1], h.buckets[12]), (2, 1, 1));
        let text = m.render();
        assert!(text.contains("# TYPE k_x_total counter\n"), "{text}");
        assert!(
            text.contains("k_x_total{direction=\"in\",method=\"/a.B/C\"} 3\n"),
            "{text}"
        );
        assert!(text.contains("k_open 2\n"), "{text}");
        assert!(
            text.contains("k_lat_seconds_bucket{method=\"m\",le=\"0.005\"} 1\n"),
            "{text}"
        );
        assert!(
            text.contains("k_lat_seconds_bucket{method=\"m\",le=\"+Inf\"} 2\n"),
            "{text}"
        );
        assert!(
            text.contains("k_lat_seconds_count{method=\"m\"} 2\n"),
            "{text}"
        );
    }

    #[test]
    fn only_the_most_recent_spans_are_kept() {
        let m = Metrics::default();
        for i in 0..300 {
            let seq = i.to_string();
            m.span(
                "k.channel.message",
                &[("seq", &seq)],
                SystemTime::now(),
                Duration::ZERO,
            );
        }
        let spans = m.spans();
        assert_eq!(spans.len(), super::SPANS_KEPT);
        assert_eq!(spans[0].attribute("seq"), Some("44"));
    }
}
