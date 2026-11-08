//! The client's address behind trusted proxies.
//!
//! `K_TRUSTED_PROXIES` (`k.server.ServerConfig.trustedProxies`) lists the
//! CIDR ranges of the reverse proxies in front of the app
//! (`docs/design/38-deployment.md`, "Behind a proxy"). Only a request whose
//! socket peer is one of them has its `X-Forwarded-For` header read, and the
//! client address is then the right-most hop that isn't a trusted proxy
//! (`docs/design/27-endpoint-pages.md`, "Client address and proxies"). With
//! the list empty, the default, the socket address is the client address and
//! forwarding headers are ignored, so a client can't choose its own address,
//! and with it its rate-limit bucket, by sending a header.
//!
//! The client address is what `RateLimit` counts guests by and what
//! `IncomingRequest.clientAddress` reports.
//!
//! Only `X-Forwarded-For` is read, as the design specifies. The standard
//! `Forwarded` header and `X-Forwarded-Proto` are ignored: whether the
//! session cookie is `Secure` follows the scheme of `K_PUBLIC_ORIGIN`, never
//! a request header.
//!
//! # Examples
//!
//! ```
//! use std::net::IpAddr;
//! use k_server::proxy::TrustedProxies;
//!
//! let proxies = TrustedProxies::parse(["10.0.0.0/8"])?;
//! let peer: IpAddr = "10.0.0.5".parse().unwrap();
//! let headers = vec![(
//!     "x-forwarded-for".to_owned(),
//!     "198.51.100.7, 203.0.113.9".to_owned(),
//! )];
//! // The proxy appended the client it saw; what the client sent is ignored.
//! assert_eq!(proxies.client_address(peer, &headers).to_string(), "203.0.113.9");
//! # Ok::<(), String>(())
//! ```

use std::net::{IpAddr, Ipv6Addr};

/// The header a trusted proxy names the client in.
pub const FORWARDED_FOR: &str = "x-forwarded-for";

/// The environment variable that lists the trusted proxies.
pub const TRUSTED_PROXIES_ENV: &str = "K_TRUSTED_PROXIES";

/// One CIDR range: an address with its host bits cleared and a prefix
/// length (`10.0.0.0/8`, `fd00::/8`, a single host as `/32` or `/128`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Cidr {
    /// The network address, host bits zero; IPv4 ranges are stored as
    /// IPv4, never as IPv4-mapped IPv6.
    network: IpAddr,
    /// The number of leading bits that must match: at most 32 for IPv4 and
    /// 128 for IPv6.
    prefix: u8,
}

impl Cidr {
    /// Parses `text`: an address, optionally followed by `/prefix`.
    ///
    /// # Errors
    ///
    /// A message naming `text` when it isn't an address, the prefix isn't a
    /// number, or the prefix is longer than the address.
    fn parse(text: &str) -> Result<Cidr, String> {
        let (addr, prefix) = match text.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (text, None),
        };
        let network: IpAddr = addr
            .parse()
            .map(canonical)
            .map_err(|_| format!("{text:?} is not an IP address or CIDR range"))?;
        let max = max_prefix(network);
        let prefix = match prefix {
            None => max,
            Some(p) => {
                let n: u8 = p
                    .parse()
                    .map_err(|_| format!("{text:?} has an invalid prefix length"))?;
                if n > max {
                    return Err(format!("{text:?} has a prefix longer than {max} bits"));
                }
                n
            }
        };
        Ok(Cidr {
            network: mask(network, prefix),
            prefix,
        })
    }

    /// Returns `true` if `ip` (already canonical) is in the range. An IPv4
    /// address is never in an IPv6 range, nor the reverse.
    fn contains(self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_)) => {
                mask(ip, self.prefix) == self.network
            }
            _ => false,
        }
    }
}

/// The trusted proxies: the parsed `K_TRUSTED_PROXIES`. Empty by default,
/// and then no forwarding header is ever read.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TrustedProxies {
    /// The ranges, in the configured order.
    ranges: Vec<Cidr>,
}

impl TrustedProxies {
    /// Parses the entries of `K_TRUSTED_PROXIES`: CIDR ranges such as
    /// `10.0.0.0/8` or `fd00::/8`, or single addresses. Surrounding
    /// whitespace and empty entries are ignored; host bits set in a range
    /// (`10.0.0.1/8`) are cleared.
    ///
    /// # Errors
    ///
    /// A message naming the first entry that isn't an address or a range,
    /// so the server can refuse to start (exit 78) instead of trusting less,
    /// or more, than was meant.
    pub fn parse<I, S>(entries: I) -> Result<TrustedProxies, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut ranges = Vec::new();
        for entry in entries {
            let entry = entry.as_ref().trim();
            if entry.is_empty() {
                continue;
            }
            ranges.push(Cidr::parse(entry)?);
        }
        Ok(TrustedProxies { ranges })
    }

    /// Reads `K_TRUSTED_PROXIES` from a resolved configuration
    /// ([`crate::config::resolve`]). A missing variable means no trusted
    /// proxies.
    ///
    /// # Errors
    ///
    /// A message for an entry that isn't an address or a range, or for a
    /// value that isn't a list of text.
    pub fn from_config(
        config: &std::collections::BTreeMap<String, crate::config::ConfigValue>,
    ) -> Result<TrustedProxies, String> {
        use crate::config::ConfigValue;
        match config.get(TRUSTED_PROXIES_ENV) {
            None | Some(ConfigValue::Nil) => Ok(TrustedProxies::default()),
            Some(ConfigValue::List(items)) => {
                let mut texts = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        ConfigValue::String(s) => texts.push(s.as_str()),
                        _ => return Err("is not a list of CIDR ranges".to_owned()),
                    }
                }
                TrustedProxies::parse(texts)
            }
            Some(ConfigValue::String(s)) => TrustedProxies::parse(s.split(',')),
            Some(_) => Err("is not a list of CIDR ranges".to_owned()),
        }
    }

    /// Returns `true` if no proxy is trusted (the default).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Returns `true` if `ip` is in one of the trusted ranges. An
    /// IPv4-mapped IPv6 address (`::ffff:10.0.0.1`) counts as its IPv4
    /// address.
    #[must_use]
    pub fn trusts(&self, ip: IpAddr) -> bool {
        let ip = canonical(ip);
        self.ranges.iter().any(|r| r.contains(ip))
    }

    /// The client address of a request from socket peer `peer` with
    /// `headers` (names lower case, in order).
    ///
    /// The peer itself unless it is a trusted proxy. Otherwise the
    /// `X-Forwarded-For` hops (every such header, in order, as one list)
    /// are walked from the right, skipping trusted proxies, and the first
    /// untrusted hop is the client. When the hops run out, or one is
    /// malformed, the last trusted address seen is the answer: nothing to
    /// its left was written by a proxy that can be believed.
    ///
    /// IPv4-mapped IPv6 addresses are reported as IPv4. Never fails.
    #[must_use]
    pub fn client_address(&self, peer: IpAddr, headers: &[(String, String)]) -> IpAddr {
        let mut current = canonical(peer);
        if !self.trusts(current) {
            return current;
        }
        let hops: Vec<&str> = headers
            .iter()
            .filter(|(n, _)| n == FORWARDED_FOR)
            .flat_map(|(_, v)| v.split(','))
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .collect();
        for hop in hops.into_iter().rev() {
            let Some(ip) = parse_hop(hop) else {
                return current;
            };
            current = ip;
            if !self.trusts(ip) {
                return ip;
            }
        }
        current
    }

    /// Returns `true` if a request from `peer` with `headers` carries an
    /// `X-Forwarded-For` header that is ignored because no proxy is
    /// trusted: the case `dist/app serve` warns about, since every client
    /// behind such a proxy shares one address and one rate limit.
    #[must_use]
    pub fn ignores_forwarding(&self, headers: &[(String, String)]) -> bool {
        self.is_empty() && headers.iter().any(|(n, _)| n == FORWARDED_FOR)
    }
}

/// Parses one `X-Forwarded-For` hop: an IPv4 or IPv6 address, an IPv6
/// address in brackets, or either with a port (`203.0.113.9:4711`,
/// `[2001:db8::1]:4711`), as some proxies write them. `None` for anything
/// else (`unknown`, an obfuscated name, a zone id, garbage).
fn parse_hop(hop: &str) -> Option<IpAddr> {
    if let Ok(ip) = hop.parse::<IpAddr>() {
        return Some(canonical(ip));
    }
    if let Some(rest) = hop.strip_prefix('[') {
        let (inside, after) = rest.split_once(']')?;
        if !(after.is_empty() || is_port_suffix(after)) {
            return None;
        }
        return inside
            .parse::<Ipv6Addr>()
            .ok()
            .map(|v6| canonical(IpAddr::V6(v6)));
    }
    let (addr, port) = hop.rsplit_once(':')?;
    if !is_port_suffix(&format!(":{port}")) {
        return None;
    }
    addr.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4)
}

/// Returns `true` for `:` followed by a port number (1 to 5 digits, at
/// most 65535).
fn is_port_suffix(s: &str) -> bool {
    s.strip_prefix(':')
        .filter(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|p| p.parse::<u16>().ok())
        .is_some()
}

/// `ip` with an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) replaced by its
/// IPv4 address, as a dual-stack socket reports IPv4 peers.
fn canonical(ip: IpAddr) -> IpAddr {
    ip.to_canonical()
}

/// The longest prefix of `ip`'s family: 32 or 128.
fn max_prefix(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    }
}

/// `ip` with every bit after the first `prefix` cleared.
fn mask(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let bits = u32::from(v4);
            let m = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
            IpAddr::V4((bits & m).into())
        }
        IpAddr::V6(v6) => {
            let bits = u128::from(v6);
            let m = u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0);
            IpAddr::V6((bits & m).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses an address literal of a test.
    fn ip(s: &str) -> IpAddr {
        s.parse().expect("an address literal")
    }

    /// `X-Forwarded-For` headers with the given values, in order.
    fn xff(values: &[&str]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|v| (FORWARDED_FOR.to_owned(), (*v).to_owned()))
            .collect()
    }

    /// Proxies trusted by the tests: a private IPv4 range, a single
    /// address and a unique-local IPv6 range.
    fn proxies() -> TrustedProxies {
        TrustedProxies::parse(["10.0.0.0/8", " 192.0.2.1 ", "fd00::/8"]).expect("valid")
    }

    #[test]
    fn parses_ranges_single_hosts_and_ipv6() {
        let p = proxies();
        assert!(p.trusts(ip("10.1.2.3")));
        assert!(p.trusts(ip("192.0.2.1")));
        assert!(!p.trusts(ip("192.0.2.2")));
        assert!(p.trusts(ip("fd12:3456::1")));
        assert!(!p.trusts(ip("fe80::1")));
        assert!(!p.trusts(ip("11.0.0.1")));
    }

    #[test]
    fn host_bits_in_a_range_are_cleared() {
        let p = TrustedProxies::parse(["10.9.9.9/8"]).expect("valid");
        assert!(p.trusts(ip("10.0.0.1")));
        assert!(!p.trusts(ip("11.0.0.1")));
    }

    #[test]
    fn zero_prefix_trusts_the_whole_family_only() {
        let p = TrustedProxies::parse(["0.0.0.0/0"]).expect("valid");
        assert!(p.trusts(ip("203.0.113.9")));
        assert!(!p.trusts(ip("2001:db8::1")));
        let p = TrustedProxies::parse(["::/0"]).expect("valid");
        assert!(p.trusts(ip("2001:db8::1")));
        assert!(!p.trusts(ip("203.0.113.9")));
    }

    #[test]
    fn empty_entries_are_ignored() {
        let p = TrustedProxies::parse(["", "  "]).expect("valid");
        assert!(p.is_empty());
    }

    #[test]
    fn malformed_entries_are_refused() {
        for bad in [
            "10.0.0.0/33",
            "fd00::/129",
            "10.0.0.0/",
            "10.0.0.0/x",
            "10.0.0.0/-1",
            "10.0.0",
            "proxy.internal",
            "10.0.0.0/8/8",
            "*",
        ] {
            let e = TrustedProxies::parse([bad]).expect_err(bad);
            assert!(e.contains(bad), "{e} names {bad}");
        }
    }

    #[test]
    fn ipv4_mapped_addresses_match_ipv4_ranges() {
        let p = proxies();
        assert!(p.trusts(ip("::ffff:10.0.0.1")));
        let peer = ip("::ffff:10.0.0.1");
        assert_eq!(
            p.client_address(peer, &xff(&["::ffff:203.0.113.9"])),
            ip("203.0.113.9")
        );
        // An untrusted mapped peer is reported as IPv4.
        assert_eq!(
            TrustedProxies::default().client_address(ip("::ffff:203.0.113.9"), &[]),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn without_trusted_proxies_the_header_is_never_read() {
        let p = TrustedProxies::default();
        let peer = ip("10.0.0.5");
        assert_eq!(p.client_address(peer, &xff(&["198.51.100.1"])), peer);
        assert!(p.ignores_forwarding(&xff(&["198.51.100.1"])));
        assert!(!p.ignores_forwarding(&[]));
        assert!(!proxies().ignores_forwarding(&xff(&["198.51.100.1"])));
    }

    #[test]
    fn an_untrusted_peer_cannot_spoof_its_address() {
        let p = proxies();
        let peer = ip("203.0.113.50");
        assert_eq!(p.client_address(peer, &xff(&["198.51.100.1"])), peer);
        assert_eq!(p.client_address(peer, &xff(&["10.0.0.1"])), peer);
    }

    #[test]
    fn the_right_most_untrusted_hop_is_the_client() {
        let p = proxies();
        let peer = ip("10.0.0.5");
        // The client sent a forged first hop; the proxy appended the real one.
        assert_eq!(
            p.client_address(peer, &xff(&["1.1.1.1, 203.0.113.9"])),
            ip("203.0.113.9")
        );
        // Two proxies: an edge one in 192.0.2.1 and an inner one in 10/8.
        assert_eq!(
            p.client_address(peer, &xff(&["6.6.6.6, 203.0.113.9, 192.0.2.1"])),
            ip("203.0.113.9")
        );
        // A client can't hide behind a trusted address it writes itself.
        assert_eq!(
            p.client_address(peer, &xff(&["10.0.0.9, 203.0.113.9"])),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn repeated_headers_are_one_list_in_order() {
        let p = proxies();
        let peer = ip("10.0.0.5");
        assert_eq!(
            p.client_address(peer, &xff(&["6.6.6.6", "203.0.113.9", "192.0.2.1"])),
            ip("203.0.113.9")
        );
        // Other headers are ignored; header names arrive lower case.
        let mut headers = xff(&["203.0.113.9"]);
        headers.insert(0, ("forwarded".to_owned(), "for=6.6.6.6".to_owned()));
        headers.push(("x-real-ip".to_owned(), "6.6.6.7".to_owned()));
        assert_eq!(p.client_address(peer, &headers), ip("203.0.113.9"));
    }

    #[test]
    fn only_trusted_hops_or_no_header_give_the_left_most_proxy() {
        let p = proxies();
        let peer = ip("10.0.0.5");
        assert_eq!(p.client_address(peer, &[]), peer);
        assert_eq!(p.client_address(peer, &xff(&[""])), peer);
        assert_eq!(
            p.client_address(peer, &xff(&["10.0.0.7, 192.0.2.1"])),
            ip("10.0.0.7")
        );
    }

    #[test]
    fn ipv6_hops_with_and_without_brackets_and_ports() {
        let p = proxies();
        let peer = ip("fd00::1");
        assert_eq!(
            p.client_address(peer, &xff(&["2001:db8::7"])),
            ip("2001:db8::7")
        );
        assert_eq!(
            p.client_address(peer, &xff(&["[2001:db8::7]"])),
            ip("2001:db8::7")
        );
        assert_eq!(
            p.client_address(peer, &xff(&["[2001:db8::7]:4711"])),
            ip("2001:db8::7")
        );
        assert_eq!(
            p.client_address(peer, &xff(&["203.0.113.9:4711"])),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn a_malformed_hop_stops_the_walk_at_the_last_trusted_address() {
        let p = proxies();
        let peer = ip("10.0.0.5");
        for bad in [
            "unknown",
            "_hidden",
            "203.0.113.9:99999",
            "203.0.113.9:",
            "[2001:db8::7",
            "[2001:db8::7]x",
            "[203.0.113.9]",
            "fe80::1%eth0",
            "2001:db8::7:4711:extra:junk:x",
            "\"203.0.113.9\"",
            "203.0.113.9 6.6.6.6",
        ] {
            assert_eq!(
                p.client_address(peer, &xff(&[&format!("6.6.6.6, {bad}")])),
                peer,
                "{bad}"
            );
            assert_eq!(
                p.client_address(peer, &xff(&[&format!("{bad}, 10.0.0.9")])),
                ip("10.0.0.9"),
                "{bad}"
            );
        }
    }

    #[test]
    fn huge_headers_do_not_panic() {
        let p = proxies();
        let peer = ip("10.0.0.5");
        let long = "10.0.0.1,".repeat(20_000);
        assert_eq!(p.client_address(peer, &xff(&[&long])), ip("10.0.0.1"));
        let junk = ",".repeat(50_000);
        assert_eq!(p.client_address(peer, &xff(&[&junk])), peer);
    }

    #[test]
    fn reads_the_resolved_configuration() {
        use crate::config::ConfigValue;
        use std::collections::BTreeMap;
        let mut config = BTreeMap::new();
        assert!(
            TrustedProxies::from_config(&config)
                .expect("absent")
                .is_empty()
        );
        config.insert(
            TRUSTED_PROXIES_ENV.to_owned(),
            ConfigValue::List(vec![ConfigValue::String("172.29.89.10/32".to_owned())]),
        );
        let p = TrustedProxies::from_config(&config).expect("valid");
        assert!(p.trusts(ip("172.29.89.10")));
        assert!(!p.trusts(ip("172.29.89.11")));
        config.insert(
            TRUSTED_PROXIES_ENV.to_owned(),
            ConfigValue::List(vec![ConfigValue::String("caddy".to_owned())]),
        );
        assert!(TrustedProxies::from_config(&config).is_err());
        config.insert(
            TRUSTED_PROXIES_ENV.to_owned(),
            ConfigValue::List(Vec::new()),
        );
        assert!(
            TrustedProxies::from_config(&config)
                .expect("empty")
                .is_empty()
        );
    }
}
