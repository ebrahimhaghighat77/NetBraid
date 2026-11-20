use std::net::IpAddr;
use std::sync::Arc;

use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::app::{self, Core};
use crate::net::rules::{Rule, BLOCK};
use crate::prefs::{LinkPrefs, Period};
use crate::snapshot::{self, FlowView, Snapshot};
use crate::{autostart, journal};

const MAX_RULES: usize = 100;
const LINKS: [(&str, &str); 4] = [
    ("github", "https://github.com/ebrahimhaghighat77"),
    ("scidev", "https://scidev.ir"),
    ("source", "https://github.com/ebrahimhaghighat77/NetBraid"),
    ("releases", "https://github.com/ebrahimhaghighat77/NetBraid/releases"),
];

fn core(app: &AppHandle) -> Arc<Core> {
    app.state::<Arc<Core>>().inner().clone()
}

/// Publishes the new state to the popup and the tray, and returns it.
fn changed(app: &AppHandle, core: &Core) -> Snapshot {
    app::publish(app, core);
    snapshot::build(core)
}

#[tauri::command]
pub fn snapshot(app: AppHandle) -> Snapshot {
    snapshot::build(&core(&app))
}

#[tauri::command]
pub async fn power(app: AppHandle, on: bool) -> Result<Snapshot, String> {
    let core = core(&app);
    let result = core.set_power(on).await;
    let snapshot = changed(&app, &core);
    result.map(|_| snapshot)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LinkChange {
    pub enabled: Option<bool>,
    pub weight: Option<u32>,
    pub backup: Option<bool>,
}

#[tauri::command]
pub fn link_update(app: AppHandle, id: String, change: LinkChange) -> Result<Snapshot, String> {
    let core = core(&app);
    let link = core.links.get(&id).ok_or("That network is gone.")?;
    core.change(|p| {
        let prefs = p.links.entry(id.clone()).or_insert_with(|| LinkPrefs::initial(link.is_virtual()));
        if let Some(enabled) = change.enabled {
            prefs.enabled = enabled;
        }
        if let Some(weight) = change.weight {
            prefs.weight = weight.clamp(1, 10);
        }
        if let Some(backup) = change.backup {
            prefs.backup = backup;
        }
    });
    if change.enabled == Some(false) {
        if let Some(flows) = core.flows() {
            flows.end_link(&id);
        }
    }
    Ok(changed(&app, &core))
}

#[tauri::command]
pub fn link_cap(app: AppHandle, id: String, cap_mb: Option<u64>, period: Period) -> Result<Snapshot, String> {
    let core = core(&app);
    let link = core.links.get(&id).ok_or("That network is gone.")?;
    core.change(|p| {
        let prefs = p.links.entry(id).or_insert_with(|| LinkPrefs::initial(link.is_virtual()));
        prefs.cap_mb = cap_mb.filter(|mb| *mb > 0);
        prefs.cap_period = period;
    });
    Ok(changed(&app, &core))
}

#[tauri::command]
pub fn usage_reset(app: AppHandle, id: String) -> Snapshot {
    let core = core(&app);
    core.usage.lock().unwrap().reset(&id);
    core.save_usage();
    changed(&app, &core)
}

/// A site as typed or pasted: no scheme, path or port, in lower case.
fn tidy_host(typed: &str) -> String {
    let host = typed.trim().to_ascii_lowercase();
    let host = host.rsplit("://").next().unwrap_or_default();
    let host = host.split(['/', '?', '#']).next().unwrap_or_default();
    match host.rsplit_once(':') {
        Some((name, port)) if !name.contains(':') && port.parse::<u16>().is_ok() => name.into(),
        _ => host.into(),
    }
}

fn tidy_rules(rules: Vec<Rule>, known: &[String]) -> Vec<Rule> {
    rules
        .into_iter()
        .map(|rule| Rule {
            app: rule.app.trim().to_owned(),
            host: tidy_host(&rule.host),
            ..rule
        })
        .filter(|r| !(r.app.is_empty() && r.host.is_empty()))
        .filter(|r| r.link == BLOCK || known.contains(&r.link))
        .take(MAX_RULES)
        .collect()
}

#[tauri::command]
pub fn rules_save(app: AppHandle, rules: Vec<Rule>) -> Snapshot {
    let core = core(&app);
    let known: Vec<String> = core.links.all().iter().map(|l| l.id.clone()).collect();
    let rules = tidy_rules(rules, &known);
    core.change(|p| p.rules = rules);
    changed(&app, &core)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OptionChange {
    pub adaptive: Option<bool>,
    pub sticky: Option<bool>,
    pub dns: Option<Vec<String>>,
    pub hide_unavailable: Option<bool>,
    pub check_updates: Option<bool>,
    pub autostart: Option<bool>,
}

fn parse_dns(typed: &[String]) -> Result<Vec<IpAddr>, String> {
    let servers: Vec<IpAddr> = typed
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().map_err(|_| format!("{s} is not an IP address.")))
        .collect::<Result<_, _>>()?;
    if servers.is_empty() {
        return Err("Add at least one DNS server.".into());
    }
    Ok(servers)
}

#[tauri::command]
pub async fn options_save(app: AppHandle, change: OptionChange) -> Result<Snapshot, String> {
    let core = core(&app);
    let dns = change.dns.as_deref().map(parse_dns).transpose()?;
    if let Some(wanted) = change.autostart {
        tauri::async_runtime::spawn_blocking(move || if wanted { autostart::enable() } else { autostart::disable() })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("Couldn't change starting with Windows: {e}"))?;
    }
    let dns_changed = dns.as_ref().is_some_and(|d| *d != core.prefs().dns);
    core.change(|p| {
        if let Some(v) = change.adaptive {
            p.adaptive = v;
        }
        if let Some(v) = change.sticky {
            p.sticky = v;
        }
        if let Some(v) = dns {
            p.dns = v;
        }
        if let Some(v) = change.hide_unavailable {
            p.hide_unavailable = v;
        }
        if let Some(v) = change.check_updates {
            p.check_updates = v;
        }
        if let Some(v) = change.autostart {
            p.autostart = v;
        }
    });
    if dns_changed {
        // The adapter takes its DNS servers as it is created.
        core.restart().await?;
    }
    Ok(changed(&app, &core))
}

#[tauri::command]
pub fn flows_list(app: AppHandle) -> Vec<FlowView> {
    snapshot::flows(&core(&app))
}

#[tauri::command]
pub fn log_lines() -> Vec<String> {
    journal::recent()
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Snapshot {
    let core = core(&app);
    app::check_for_update(&core).await;
    changed(&app, &core)
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let core = core(&app);
    let result = app::install_update(&app, &core).await;
    app::publish(&app, &core);
    result
}

#[tauri::command]
pub fn open_link(target: String) -> Result<(), String> {
    let (_, url) = LINKS.iter().find(|(name, _)| *name == target).ok_or("Unknown link.")?;
    // Started through Explorer, so the browser doesn't inherit administrator rights.
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map(drop)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_tidied_as_typed() {
        assert_eq!(tidy_host(" https://WWW.Example.com/a?b "), "www.example.com");
        assert_eq!(tidy_host("example.com:8443"), "example.com");
        assert_eq!(tidy_host("2001:db8::1"), "2001:db8::1");
    }

    #[test]
    fn rules_need_a_target_and_a_known_network() {
        let rule = |app: &str, host: &str, link: &str| Rule {
            app: app.into(),
            host: host.into(),
            link: link.into(),
            fallback: false,
        };
        let known = vec!["wifi".to_owned()];
        let kept = tidy_rules(
            vec![rule(" game.exe ", "", "wifi"), rule("", "", "wifi"), rule("a.exe", "", "gone"), rule("", "ads.example", BLOCK)],
            &known,
        );
        assert_eq!(kept, vec![rule("game.exe", "", "wifi"), rule("", "ads.example", BLOCK)]);
    }

    #[test]
    fn dns_servers_must_be_addresses() {
        assert_eq!(parse_dns(&["1.1.1.1".into(), " ".into()]).unwrap().len(), 1);
        assert!(parse_dns(&["dns.google".into()]).is_err());
        assert!(parse_dns(&[]).is_err());
    }
}
