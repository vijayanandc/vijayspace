//! Which destination addresses outbound requests may reach.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Blocks private, loopback, link-local (cloud metadata), CGNAT and other non-public ranges,
/// unless private networks are explicitly allowed (development, self-hosting).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsrfPolicy {
    allow_private: bool,
}

impl SsrfPolicy {
    /// Production default: public addresses only.
    #[must_use]
    pub const fn public_only() -> Self {
        Self {
            allow_private: false,
        }
    }

    /// Development and tests: everything allowed (e.g. a local Appblocks or a mock server).
    #[must_use]
    pub const fn allow_private() -> Self {
        Self {
            allow_private: true,
        }
    }

    #[must_use]
    pub const fn allows_private(&self) -> bool {
        self.allow_private
    }

    /// Whether a resolved destination address may be contacted.
    #[must_use]
    pub fn allows(&self, ip: IpAddr) -> bool {
        self.allow_private || is_public(ip)
    }
}

fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or_else(|| is_public_v6(v6), is_public_v4),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0b1100_0000) == 64) // 100.64.0.0/10 CGNAT
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // reserved
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (seg[0] & 0xfe00) == 0xfc00 // fc00::/7 unique local
        || (seg[0] & 0xffc0) == 0xfe80 // fe80::/10 link local
        || (seg[0] == 0x2001 && seg[1] == 0x0db8)) // documentation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_non_public_ranges() {
        let p = SsrfPolicy::public_only();
        for blocked in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            assert!(
                !p.allows(blocked.parse().unwrap()),
                "{blocked} should be blocked"
            );
        }
        for allowed in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(
                p.allows(allowed.parse().unwrap()),
                "{allowed} should be allowed"
            );
        }
        assert!(SsrfPolicy::allow_private().allows("127.0.0.1".parse().unwrap()));
    }
}
