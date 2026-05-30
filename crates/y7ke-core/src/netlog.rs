//! Structured networking-lifecycle log categories. One `cat=` field on
//! every networking line so logs are greppable by phase. Reuses the
//! process-wide `tracing` subscriber; defines no subscriber and pulls no
//! libp2p dep (this is the leaf crate). Multiaddr-derived facts (ip
//! family) are computed in y7ke-net / y7ke-app, never here.

/// Networking lifecycle category. Rendered as the `cat` structured field.
///
/// NOTE: `cat` is an event field, so it is GREP-able (`grep cat=DCUTR`),
/// NOT EnvFilter-selectable (EnvFilter field matching applies to spans).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat {
    Discovery,
    Transport,
    Dcutr,
    Relay,
    Connection,
    IpVersion,
    Autonat,
}

impl Cat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Cat::Discovery => "DISCOVERY",
            Cat::Transport => "TRANSPORT",
            Cat::Dcutr => "DCUTR",
            Cat::Relay => "RELAY",
            Cat::Connection => "CONNECTION",
            Cat::IpVersion => "IPVERSION",
            Cat::Autonat => "AUTONAT",
        }
    }
}

impl std::fmt::Display for Cat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `netlog!(info, Cat::Connection, peer = %p, "established")`
///
/// Expands to a `tracing::event!` carrying `cat = "<CATEGORY>"` as the first
/// field. Level is one of the `tracing` level idents (trace/debug/info/warn/
/// error). The macro (not a fn) preserves `tracing`'s compile-time level
/// filtering and call-site location.
#[macro_export]
macro_rules! netlog {
    ($lvl:ident, $cat:expr, $($rest:tt)+) => {
        ::tracing::$lvl!(cat = $cat.as_str(), $($rest)+)
    };
}
