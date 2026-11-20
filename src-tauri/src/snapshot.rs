use std::collections::HashMap;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use serde::Serialize;

use crate::app::Core;
use crate::engine::TUN_ADDR;
use crate::net::link::{Kind, Reach};
use crate::net::rules::Rule;
use crate::prefs::Period;
use crate::update::{Stage, VERSION};
use crate::win;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: Kind,
    pub address: Option<String>,
    pub gateway: Option<String>,
    pub latency_ms: Option<u32>,
    pub reach: Reach,
    pub present: bool,
    pub reason: &'static str,
    pub virtual_adapter: bool,
    pub is_default: bool,
    pub enabled: bool,
    pub weight: u32,
    pub backup: bool,
    pub cap_mb: Option<u64>,
    pub cap_period: Period,
    pub used_bytes: u64,
    pub capped: bool,
    pub rx_rate: u64,
    pub tx_rate: u64,
    pub share: u8,
    pub connections: u32,
    pub history: Vec<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub adaptive: bool,
    pub sticky: bool,
    pub dns: Vec<String>,
    pub rules: Vec<Rule>,
    pub hide_unavailable: bool,
    pub check_updates: bool,
    pub autostart: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    pub stage: Stage,
    pub latest: Option<String>,
    pub notes: Option<String>,
    pub progress: Option<u8>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: &'static str,
    pub on: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub rx_rate: u64,
    pub tx_rate: u64,
    pub connections: usize,
    pub today_bytes: u64,
    pub links: Vec<LinkView>,
    pub settings: SettingsView,
    pub update: UpdateView,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowView {
    pub id: u64,
    pub udp: bool,
    pub app: Option<String>,
    pub host: Option<String>,
    pub destination: String,
    pub link_id: Option<String>,
    pub link_name: Option<String>,
    pub rx: u64,
    pub tx: u64,
    pub rx_rate: u64,
    pub tx_rate: u64,
    pub seconds: u64,
}

/// Below this many bytes a second, shares follow connections instead of traffic.
const IDLE_RATE: u64 = 4 * 1024;

fn shares(values: &[u64]) -> Vec<u8> {
    let total: u64 = values.iter().sum();
    values
        .iter()
        .map(|v| (v * 100 + total / 2).checked_div(total).unwrap_or(0) as u8)
        .collect()
}

pub fn build(core: &Core) -> Snapshot {
    let prefs = core.prefs();
    let today = win::local_date();
    let usage = core.usage.lock().unwrap().clone();
    let default_id = core.links.default_id();
    let mut links: Vec<LinkView> = core
        .links
        .all()
        .iter()
        .map(|link| {
            let info = link.info();
            let p = prefs.link(&link.id, info.virtual_adapter);
            let (rx_rate, tx_rate) = link.rates();
            LinkView {
                id: link.id.clone(),
                name: info.name,
                description: info.description,
                kind: info.kind,
                address: info.ipv4.map(|ip| ip.to_string()),
                gateway: info.gateway.map(|ip| ip.to_string()),
                latency_ms: link.latency_ms(),
                reach: link.reach(),
                present: info.present,
                reason: info.reason,
                virtual_adapter: info.virtual_adapter,
                is_default: default_id.as_deref() == Some(link.id.as_str()),
                enabled: p.enabled,
                weight: p.weight,
                backup: p.backup,
                cap_mb: p.cap_mb,
                cap_period: p.cap_period,
                used_bytes: usage.used(&link.id, p.period_of(today)),
                capped: link.capped(),
                rx_rate,
                tx_rate,
                share: 0,
                connections: link.open_flows(),
                history: link.history(),
            }
        })
        .collect();
    let mut basis: Vec<u64> = links.iter().map(|l| l.rx_rate).collect();
    if basis.iter().sum::<u64>() < IDLE_RATE {
        basis = links.iter().map(|l| u64::from(l.connections)).collect();
    }
    for (link, share) in links.iter_mut().zip(shares(&basis)) {
        link.share = share;
    }
    links.sort_by(|a, b| {
        (!a.is_default, !a.present, a.name.to_lowercase()).cmp(&(!b.is_default, !b.present, b.name.to_lowercase()))
    });

    let flows = core.flows();
    let update = core.update.lock().unwrap().clone();
    Snapshot {
        version: VERSION,
        on: flows.is_some(),
        busy: core.busy.load(Relaxed),
        error: core.error.lock().unwrap().clone(),
        rx_rate: links.iter().map(|l| l.rx_rate).sum(),
        tx_rate: links.iter().map(|l| l.tx_rate).sum(),
        connections: flows.map_or(0, |f| f.len()),
        today_bytes: usage.today(today),
        links,
        settings: SettingsView {
            adaptive: prefs.adaptive,
            sticky: prefs.sticky,
            dns: prefs.dns.iter().map(|d| d.to_string()).collect(),
            rules: prefs.rules.clone(),
            hide_unavailable: prefs.hide_unavailable,
            check_updates: prefs.check_updates,
            autostart: prefs.autostart,
        },
        update: UpdateView {
            stage: update.stage,
            latest: update.release.as_ref().map(|r| r.version.clone()),
            notes: update.release.as_ref().map(|r| r.notes.clone()),
            progress: update.progress,
            error: update.error,
        },
    }
}

pub fn tooltip(snapshot: &Snapshot) -> String {
    if !snapshot.on {
        return "NetBraid is off".into();
    }
    let working = snapshot
        .links
        .iter()
        .filter(|l| l.present && l.enabled && !l.capped && l.reach != Reach::Down)
        .count();
    match working {
        1 => "NetBraid is on, using 1 network".into(),
        n => format!("NetBraid is combining {n} networks"),
    }
}

/// Every open connection, busiest first.
pub fn flows(core: &Core) -> Vec<FlowView> {
    let Some(table) = core.flows() else {
        return Vec::new();
    };
    let list = table.list();
    let owners = win::tun_socket_owners(TUN_ADDR);
    let mut names: HashMap<u32, Option<String>> = HashMap::new();
    let link_names: HashMap<String, String> =
        core.links.all().iter().map(|l| (l.id.clone(), l.name())).collect();
    let now = Instant::now();
    let mut seen = core.flow_seen.lock().unwrap();
    let elapsed = seen.0.map(|at| now.duration_since(at).as_secs_f64()).filter(|s| *s > 0.0);
    let mut counts = HashMap::with_capacity(list.len());
    let mut views: Vec<FlowView> = list
        .iter()
        .map(|flow| {
            let (rx, tx) = (flow.rx.load(Relaxed), flow.tx.load(Relaxed));
            counts.insert(flow.id, (rx, tx));
            let (rx_before, tx_before) = seen.1.get(&flow.id).copied().unwrap_or((rx, tx));
            let rate = |now: u64, before: u64| elapsed.map_or(0, |s| (now.saturating_sub(before) as f64 / s) as u64);
            let app = flow.app.get().cloned().flatten().or_else(|| {
                let pid = *owners.get(&(!flow.udp, flow.src.port()))?;
                names.entry(pid).or_insert_with(|| win::process_name(pid)).clone()
            });
            let link_id = flow.link.get().cloned();
            FlowView {
                id: flow.id,
                udp: flow.udp,
                app,
                host: flow.host.get().cloned(),
                destination: flow.dst.to_string(),
                link_name: link_id.as_ref().and_then(|id| link_names.get(id).cloned()),
                link_id,
                rx,
                tx,
                rx_rate: rate(rx, rx_before),
                tx_rate: rate(tx, tx_before),
                seconds: flow.opened.elapsed().as_secs(),
            }
        })
        .collect();
    *seen = (Some(now), counts);
    views.sort_by_key(|v| std::cmp::Reverse((v.rx_rate + v.tx_rate, v.rx + v.tx)));
    views
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_are_rounded_percentages() {
        assert_eq!(shares(&[3, 1]), vec![75, 25]);
        assert_eq!(shares(&[1, 1, 1]), vec![33, 33, 33]);
        assert_eq!(shares(&[0, 0]), vec![0, 0]);
    }
}
