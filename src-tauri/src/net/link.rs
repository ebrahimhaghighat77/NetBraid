use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, RwLock};

use serde::Serialize;

use crate::engine::TUN_NAME;
use crate::win;

pub const HISTORY: usize = 60;
const MISSES_TO_DOWN: u32 = 2;
const IF_TYPE_ETHERNET: u32 = 6;
const IF_TYPE_LOOPBACK: u32 = 24;
const IF_TYPE_WIFI: u32 = 71;
const IF_TYPE_WWAN: [u32; 2] = [243, 244];
const TETHER_HINTS: [&str; 5] = ["ndis", "mobile device", "iphone", "android", "tether"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Ethernet,
    Wifi,
    Phone,
    Cellular,
    #[default]
    Other,
}

/// How far a network gets: everywhere, only to addresses inside the
/// country, or nowhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Reach {
    #[default]
    Unknown,
    Full,
    Local,
    Down,
}

#[derive(Clone, Debug, Default)]
pub struct LinkInfo {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub kind: Kind,
    pub ipv4: Option<Ipv4Addr>,
    pub gateway: Option<Ipv4Addr>,
    pub dns: Vec<IpAddr>,
    pub present: bool,
    pub virtual_adapter: bool,
    pub reason: &'static str,
}

#[derive(Default)]
struct Health {
    reach: Reach,
    latency_ms: Option<u32>,
    misses: u32,
}

#[derive(Default)]
struct Rates {
    rx: u64,
    tx: u64,
    seen_rx: u64,
    seen_tx: u64,
    history: VecDeque<u64>,
}

#[derive(Default)]
pub struct Link {
    pub id: String,
    info: RwLock<LinkInfo>,
    health: Mutex<Health>,
    rates: Mutex<Rates>,
    rx: AtomicU64,
    tx: AtomicU64,
    open_tcp: AtomicU32,
    open_udp: AtomicU32,
    peak: AtomicU64,
    busy: AtomicU32,
    fresh: AtomicU32,
    capped: AtomicBool,
    probed_rx: AtomicU64,
}

impl Link {
    pub fn new(id: String, info: LinkInfo) -> Link {
        Link {
            id,
            info: RwLock::new(info),
            ..Link::default()
        }
    }

    pub fn info(&self) -> LinkInfo {
        self.info.read().unwrap().clone()
    }

    pub fn name(&self) -> String {
        self.info.read().unwrap().name.clone()
    }

    pub fn ipv4(&self) -> Option<Ipv4Addr> {
        self.info.read().unwrap().ipv4
    }

    pub fn is_virtual(&self) -> bool {
        self.info.read().unwrap().virtual_adapter
    }

    pub fn present(&self) -> bool {
        self.info.read().unwrap().present
    }

    pub fn reach(&self) -> Reach {
        self.health.lock().unwrap().reach
    }

    pub fn latency_ms(&self) -> Option<u32> {
        self.health.lock().unwrap().latency_ms
    }

    pub fn down(&self) -> bool {
        self.reach() == Reach::Down
    }

    pub fn limited(&self) -> bool {
        self.reach() == Reach::Local
    }

    pub fn capped(&self) -> bool {
        self.capped.load(Relaxed)
    }

    pub fn set_capped(&self, capped: bool) {
        if self.capped.swap(capped, Relaxed) != capped {
            let state = if capped { "reached its data cap" } else { "is under its data cap again" };
            log::info!("{} {state}", self.name());
        }
    }

    pub fn usable(&self) -> bool {
        self.present() && !self.capped() && !self.down()
    }

    pub fn peak(&self) -> u64 {
        self.peak.load(Relaxed)
    }

    /// Flows moving data now, plus the ones opened since the last count.
    pub fn busy(&self) -> u32 {
        self.busy.load(Relaxed) + self.fresh.load(Relaxed)
    }

    pub fn set_busy(&self, busy: u32) {
        self.busy.store(busy, Relaxed);
        self.fresh.store(0, Relaxed);
    }

    pub fn opened(&self, udp: bool) {
        self.counter(udp).fetch_add(1, Relaxed);
        self.fresh.fetch_add(1, Relaxed);
    }

    pub fn closed(&self, udp: bool) {
        self.counter(udp).fetch_sub(1, Relaxed);
    }

    fn counter(&self, udp: bool) -> &AtomicU32 {
        if udp {
            &self.open_udp
        } else {
            &self.open_tcp
        }
    }

    pub fn open_flows(&self) -> u32 {
        self.open_tcp.load(Relaxed) + self.open_udp.load(Relaxed)
    }

    pub fn add_rx(&self, n: u64) {
        self.rx.fetch_add(n, Relaxed);
    }

    pub fn add_tx(&self, n: u64) {
        self.tx.fetch_add(n, Relaxed);
    }

    /// Everything the network has carried, both ways.
    pub fn carried(&self) -> u64 {
        self.rx.load(Relaxed) + self.tx.load(Relaxed)
    }

    pub fn rates(&self) -> (u64, u64) {
        let r = self.rates.lock().unwrap();
        (r.rx, r.tx)
    }

    pub fn history(&self) -> Vec<u64> {
        self.rates.lock().unwrap().history.iter().copied().collect()
    }

    pub fn rx_since_probe(&self) -> u64 {
        let rx = self.rx.load(Relaxed);
        rx.saturating_sub(self.probed_rx.swap(rx, Relaxed))
    }

    pub fn heard(&self, reach: Reach, latency_ms: Option<u32>) {
        let mut h = self.health.lock().unwrap();
        if h.reach != reach {
            let what = match reach {
                Reach::Full => "reaches the internet",
                Reach::Local => "only reaches addresses inside the country",
                _ => "changed state",
            };
            log::info!("{} {what}", self.name());
        }
        h.reach = reach;
        h.misses = 0;
        if latency_ms.is_some() {
            h.latency_ms = latency_ms;
        }
    }

    /// Records a round with no answer; true once that makes it down.
    pub fn missed(&self) -> bool {
        let mut h = self.health.lock().unwrap();
        h.misses += 1;
        if h.misses >= MISSES_TO_DOWN && h.reach != Reach::Down {
            log::warn!("{} has no internet: nothing answered", self.name());
            h.reach = Reach::Down;
            h.latency_ms = None;
        }
        h.reach == Reach::Down
    }

    fn tick(&self) {
        let (rx, tx) = (self.rx.load(Relaxed), self.tx.load(Relaxed));
        let now = {
            let mut r = self.rates.lock().unwrap();
            r.rx = rx.saturating_sub(r.seen_rx);
            r.tx = tx.saturating_sub(r.seen_tx);
            r.seen_rx = rx;
            r.seen_tx = tx;
            let now = r.rx;
            r.history.push_back(now);
            while r.history.len() > HISTORY {
                r.history.pop_front();
            }
            now
        };
        // Falls to half in about 22 s, so the estimate follows a network that slowed down.
        let peak = self.peak.load(Relaxed);
        self.peak.store((peak - peak / 32).max(now), Relaxed);
    }
}

#[derive(Default)]
pub struct LinkSet {
    list: RwLock<Vec<Arc<Link>>>,
    default_index: AtomicU32,
}

impl LinkSet {
    pub fn all(&self) -> Vec<Arc<Link>> {
        self.list.read().unwrap().clone()
    }

    pub fn get(&self, id: &str) -> Option<Arc<Link>> {
        self.list.read().unwrap().iter().find(|l| l.id == id).cloned()
    }

    /// The network Windows itself sends through, as last seen.
    pub fn default_id(&self) -> Option<String> {
        let index = self.default_index.load(Relaxed);
        let list = self.list.read().unwrap();
        list.iter().find(|l| l.info.read().unwrap().index == index).map(|l| l.id.clone())
    }

    /// Reads the adapters again; links keep their counters across changes.
    /// True when anything changed.
    pub fn refresh(&self) -> bool {
        let routed = win::default_route_ifaces();
        let found = discover(&routed);
        if let Some(index) = routed.iter().find(|i| found.iter().any(|(_, f)| f.index == **i)) {
            self.default_index.store(*index, Relaxed);
        }
        let mut list = self.list.write().unwrap();
        let mut changed = false;
        for link in list.iter() {
            let mut info = link.info.write().unwrap();
            match found.iter().find(|(id, _)| *id == link.id) {
                Some((_, fresh)) => {
                    if info.present != fresh.present || info.ipv4 != fresh.ipv4 {
                        *link.health.lock().unwrap() = Health::default();
                        changed = true;
                    }
                    *info = fresh.clone();
                }
                None if info.present => {
                    info.present = false;
                    info.reason = "unplugged";
                    changed = true;
                }
                None => {}
            }
        }
        for (id, info) in found {
            if !list.iter().any(|l| l.id == id) {
                list.push(Arc::new(Link::new(id, info)));
                changed = true;
            }
        }
        if changed {
            for link in list.iter() {
                let info = link.info.read().unwrap();
                let state = if info.present { "available" } else { info.reason };
                let address = info.ipv4.map(|ip| ip.to_string()).unwrap_or_default();
                log::info!("network {} ({:?}) {address}: {state}", info.name, info.kind);
            }
        }
        changed
    }

    pub fn tick(&self) {
        for link in self.all() {
            link.tick();
        }
    }

    #[cfg(test)]
    pub fn with(list: Vec<Arc<Link>>) -> LinkSet {
        LinkSet {
            list: RwLock::new(list),
            default_index: AtomicU32::new(0),
        }
    }
}

fn kind_of(adapter: &win::Adapter) -> Kind {
    let description = adapter.description.to_lowercase();
    if TETHER_HINTS.iter().any(|hint| description.contains(hint)) {
        return Kind::Phone;
    }
    match adapter.if_type {
        IF_TYPE_WIFI => Kind::Wifi,
        IF_TYPE_ETHERNET => Kind::Ethernet,
        t if IF_TYPE_WWAN.contains(&t) => Kind::Cellular,
        _ => Kind::Other,
    }
}

fn discover(routed: &[u32]) -> Vec<(String, LinkInfo)> {
    win::list_adapters()
        .into_iter()
        .filter(|a| a.if_type != IF_TYPE_LOOPBACK && a.name != TUN_NAME)
        .filter_map(|a| {
            let ipv4 = a
                .ipv4
                .iter()
                .copied()
                .find(|ip| !ip.is_link_local() && !ip.is_unspecified() && !ip.is_loopback());
            let gateway = a.gateways_v4.first().copied();
            let present = a.up && ipv4.is_some() && (gateway.is_some() || routed.contains(&a.index));
            let kind = kind_of(&a);
            let worth_listing = a.hardware
                && kind != Kind::Other
                && !a.description.to_lowercase().contains("bluetooth");
            if !present && !worth_listing {
                return None;
            }
            let reason = match (present, a.up, ipv4) {
                (true, ..) => "",
                (_, false, _) => "not connected",
                (_, _, None) => "no IP address",
                _ => "no route to the internet",
            };
            let dns = a.dns.iter().copied().filter(|ip| ip.is_ipv4() && !ip.is_unspecified()).collect();
            let info = LinkInfo {
                index: a.index,
                name: a.name.clone(),
                description: a.description.clone(),
                kind,
                ipv4,
                gateway,
                dns,
                present,
                virtual_adapter: !a.hardware,
                reason,
            };
            Some((a.guid, info))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_misses_take_a_link_down_and_an_answer_brings_it_back() {
        let link = Link::new("a".into(), LinkInfo::default());
        assert!(!link.missed());
        assert!(link.missed());
        assert!(link.down());
        link.heard(Reach::Local, Some(40));
        assert!(link.limited() && !link.down());
        assert_eq!(link.latency_ms(), Some(40));
    }

    #[test]
    fn rates_come_from_counters_and_the_peak_decays() {
        let link = Link::new("a".into(), LinkInfo::default());
        link.add_rx(64_000);
        link.tick();
        assert_eq!(link.rates().0, 64_000);
        assert_eq!(link.peak(), 64_000);
        link.tick();
        assert_eq!(link.rates().0, 0);
        assert_eq!(link.peak(), 62_000);
        assert_eq!(link.history(), vec![64_000, 0]);
    }

    #[test]
    fn busy_counts_new_flows_until_the_next_count() {
        let link = Link::new("a".into(), LinkInfo::default());
        link.opened(false);
        link.opened(true);
        assert_eq!((link.busy(), link.open_flows()), (2, 2));
        link.set_busy(1);
        link.closed(true);
        assert_eq!((link.busy(), link.open_flows()), (1, 1));
    }
}
