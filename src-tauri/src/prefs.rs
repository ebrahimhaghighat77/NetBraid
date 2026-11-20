use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::net::pick::{LinkPolicy, Policy};
use crate::net::rules::Rule;
use crate::usage::Day;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Period {
    Day,
    #[default]
    Month,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LinkPrefs {
    pub enabled: bool,
    /// Share of new connections, 1 to 10.
    pub weight: u32,
    pub backup: bool,
    /// Decimal megabytes per period; none means no cap.
    pub cap_mb: Option<u64>,
    pub cap_period: Period,
}

impl Default for LinkPrefs {
    fn default() -> Self {
        LinkPrefs {
            enabled: true,
            weight: 5,
            backup: false,
            cap_mb: None,
            cap_period: Period::Month,
        }
    }
}

impl LinkPrefs {
    /// A VPN's or other virtual adapter stays off until switched on.
    pub fn initial(virtual_adapter: bool) -> LinkPrefs {
        LinkPrefs {
            enabled: !virtual_adapter,
            ..LinkPrefs::default()
        }
    }

    pub fn cap_bytes(&self) -> Option<u64> {
        self.cap_mb.map(|mb| mb.saturating_mul(1_000_000))
    }

    /// The day, or the month with day 0, that usage counts towards.
    pub fn period_of(&self, today: Day) -> Day {
        match self.cap_period {
            Period::Day => today,
            Period::Month => (today.0, today.1, 0),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Prefs {
    /// It was on when the app last ran, so it turns on again at start.
    pub on: bool,
    /// By adapter GUID, which survives renames and reboots.
    pub links: HashMap<String, LinkPrefs>,
    pub adaptive: bool,
    pub sticky: bool,
    pub dns: Vec<IpAddr>,
    pub rules: Vec<Rule>,
    pub hide_unavailable: bool,
    pub check_updates: bool,
    pub autostart: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            on: false,
            links: HashMap::new(),
            adaptive: true,
            sticky: false,
            dns: vec![IpAddr::from([1, 1, 1, 1]), IpAddr::from([8, 8, 8, 8])],
            rules: Vec::new(),
            hide_unavailable: false,
            check_updates: true,
            autostart: false,
        }
    }
}

impl Prefs {
    pub fn link(&self, id: &str, virtual_adapter: bool) -> LinkPrefs {
        self.links
            .get(id)
            .cloned()
            .unwrap_or_else(|| LinkPrefs::initial(virtual_adapter))
    }

    pub fn policy(&self) -> Policy {
        Policy {
            links: self
                .links
                .iter()
                .map(|(id, p)| {
                    let policy = LinkPolicy {
                        enabled: p.enabled,
                        weight: p.weight.clamp(1, 10),
                        backup: p.backup,
                    };
                    (id.clone(), policy)
                })
                .collect(),
            adaptive: self.adaptive,
            sticky: self.sticky,
            rules: self.rules.clone(),
        }
    }
}

pub fn read<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes next to the file first, so a crash never leaves half of it.
pub fn write<T: Serialize>(path: &Path, value: &T) {
    let Ok(json) = serde_json::to_vec_pretty(value) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let partial = path.with_extension("partial");
    if std::fs::write(&partial, json).is_ok() {
        let _ = std::fs::rename(&partial, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_take_their_defaults() {
        let prefs: Prefs = serde_json::from_str(r#"{"links":{"a":{"weight":8}}}"#).unwrap();
        assert!(prefs.adaptive && prefs.check_updates && !prefs.on);
        let a = prefs.link("a", false);
        assert_eq!((a.weight, a.enabled, a.cap_mb), (8, true, None));
        assert!(!prefs.link("vpn", true).enabled);
    }

    #[test]
    fn caps_count_per_day_or_month() {
        let mut link = LinkPrefs {
            cap_mb: Some(500),
            ..LinkPrefs::default()
        };
        assert_eq!(link.cap_bytes(), Some(500_000_000));
        assert_eq!(link.period_of((2026, 11, 20)), (2026, 11, 0));
        link.cap_period = Period::Day;
        assert_eq!(link.period_of((2026, 11, 20)), (2026, 11, 20));
    }

    #[test]
    fn the_policy_clamps_weights_and_keeps_rules() {
        let prefs: Prefs = serde_json::from_str(
            r#"{"links":{"a":{"weight":40,"backup":true}},"rules":[{"app":"game.exe","link":"a"}]}"#,
        )
        .unwrap();
        let policy = prefs.policy();
        assert_eq!(policy.links["a"].weight, 10);
        assert!(policy.links["a"].backup);
        assert_eq!(policy.rules[0].app, "game.exe");
    }
}
