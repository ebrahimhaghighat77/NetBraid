use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use tokio::net::{TcpStream, UdpSocket};
use tokio::task::JoinSet;

use super::dial;
use super::link::{Link, LinkSet};
use super::rules::{self, Rule, Verdict};

const CONNECT_LIMIT: Duration = Duration::from_secs(8);
/// How long one network gets before the next one dials alongside it.
const RACE_AFTER_MIN: Duration = Duration::from_millis(300);
const RACE_AFTER_MAX: Duration = Duration::from_millis(1500);
const STICKY_FOR: Duration = Duration::from_secs(600);
const STICKY_MAX: usize = 4096;
/// A peak below this, in bytes/s, says nothing about a network's speed.
const MEASURED: u64 = 200 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkPolicy {
    pub enabled: bool,
    pub weight: u32,
    pub backup: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Policy {
    pub links: HashMap<String, LinkPolicy>,
    pub adaptive: bool,
    pub sticky: bool,
    pub rules: Vec<Rule>,
}

impl Policy {
    pub fn link(&self, link: &Link) -> LinkPolicy {
        self.links.get(&link.id).copied().unwrap_or(LinkPolicy {
            enabled: !link.is_virtual(),
            weight: 5,
            backup: false,
        })
    }
}

/// The network a rule asks for, and whether the others may stand in for it.
pub type LinkPin<'a> = Option<(&'a str, bool)>;

/// Counts a flow against its network for as long as it lives.
pub struct Lease {
    pub link: Arc<Link>,
    udp: bool,
}

impl Lease {
    pub fn open(link: Arc<Link>, udp: bool) -> Lease {
        link.opened(udp);
        Lease { link, udp }
    }

    pub fn sent(&self, n: usize) {
        self.link.add_tx(n as u64);
    }

    pub fn received(&self, n: usize) {
        self.link.add_rx(n as u64);
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.link.closed(self.udp);
    }
}

pub struct Picker {
    links: Arc<LinkSet>,
    policy: RwLock<Policy>,
    sticky: Mutex<HashMap<String, (String, Instant)>>,
}

fn no_network() -> io::Error {
    io::Error::new(io::ErrorKind::NetworkUnreachable, "no network can carry this connection")
}

impl Picker {
    pub fn new(links: Arc<LinkSet>, policy: Policy) -> Picker {
        Picker {
            links,
            policy: RwLock::new(policy),
            sticky: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_policy(&self, policy: Policy) {
        if !policy.sticky {
            self.sticky.lock().unwrap().clear();
        }
        *self.policy.write().unwrap() = policy;
    }

    pub fn enabled(&self, link: &Link) -> bool {
        self.policy.read().unwrap().link(link).enabled
    }

    pub fn has_app_rules(&self) -> bool {
        self.policy.read().unwrap().rules.iter().any(|r| !r.app.is_empty())
    }

    pub fn verdict(&self, app: Option<&str>, host: Option<&str>, ip: IpAddr) -> Verdict {
        rules::verdict(&self.policy.read().unwrap().rules, app, host, ip)
    }

    /// The networks a new flow may use, with their weights. Working networks
    /// come before failing ones, those reaching everything before those only
    /// reaching the country, and backups last.
    fn candidates(&self, policy: &Policy, exclude: &[String]) -> Vec<(Arc<Link>, u32)> {
        let mut found: Vec<(Arc<Link>, u32, (bool, bool, bool))> = self
            .links
            .all()
            .into_iter()
            .filter(|l| !exclude.contains(&l.id) && l.present() && !l.capped() && l.ipv4().is_some())
            .filter_map(|l| {
                let p = policy.link(&l);
                let tier = (l.down(), l.limited(), p.backup);
                p.enabled.then_some((l, p.weight.max(1), tier))
            })
            .collect();
        let best = found.iter().map(|f| f.2).min();
        found.retain(|f| Some(f.2) == best);
        found.into_iter().map(|(l, w, _)| (l, w)).collect()
    }

    /// Every network a lookup is sent through at once.
    pub fn spread(&self) -> Vec<Arc<Link>> {
        let policy = self.policy.read().unwrap();
        self.candidates(&policy, &[]).into_iter().map(|(l, _)| l).collect()
    }

    /// The network where one more flow can expect the most room:
    /// fewest busy flows for its share of weight and measured speed.
    fn least_loaded(policy: &Policy, list: &[(Arc<Link>, u32)]) -> Option<Arc<Link>> {
        let measured: Vec<f64> = list
            .iter()
            .map(|(l, _)| l.peak())
            .filter(|p| *p >= MEASURED)
            .map(|p| p as f64)
            .collect();
        let typical = measured.iter().sum::<f64>() / measured.len().max(1) as f64;
        list.iter()
            .map(|(link, weight)| {
                let peak = link.peak() as f64;
                let speed = if !policy.adaptive || measured.is_empty() {
                    1.0
                } else if link.peak() < MEASURED || link.busy() == 0 {
                    // An idle network only shows what it carried, not what it could.
                    peak.max(typical)
                } else {
                    peak
                };
                let load = (link.busy() as f64 + 1.0) / (*weight as f64 * speed);
                (load, link.latency_ms().unwrap_or(u32::MAX), link)
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.id.cmp(&b.2.id)))
            .map(|(_, _, link)| link.clone())
    }

    pub fn choose(&self, key: Option<&str>, exclude: &[String], pin: LinkPin) -> Option<Arc<Link>> {
        let policy = self.policy.read().unwrap();
        if let Some((id, fallback)) = pin {
            let pinned = self.links.get(id).filter(|l| {
                !exclude.iter().any(|e| e == id)
                    && l.present()
                    && !l.capped()
                    && l.ipv4().is_some()
                    && policy.link(l).enabled
            });
            if pinned.is_some() || !fallback {
                return pinned;
            }
        }
        let list = self.candidates(&policy, exclude);
        let key = key.filter(|_| policy.sticky);
        if let Some(key) = key {
            let sticky = self.sticky.lock().unwrap();
            if let Some((id, at)) = sticky.get(key) {
                if at.elapsed() < STICKY_FOR {
                    if let Some((link, _)) = list.iter().find(|(l, _)| &l.id == id) {
                        return Some(link.clone());
                    }
                }
            }
        }
        let link = Self::least_loaded(&policy, &list)?;
        if let Some(key) = key {
            let mut sticky = self.sticky.lock().unwrap();
            if sticky.len() >= STICKY_MAX {
                sticky.retain(|_, (_, at)| at.elapsed() < STICKY_FOR);
            }
            sticky.insert(key.to_owned(), (link.id.clone(), Instant::now()));
        }
        Some(link)
    }

    /// Dials through the best network, and through the next one alongside it
    /// whenever those under way are slow or fail; the first to connect wins.
    /// Sites kept on one network wait for their own instead.
    pub async fn connect_tcp(
        &self,
        to: SocketAddr,
        key: Option<&str>,
        pin: LinkPin<'_>,
    ) -> io::Result<(Lease, TcpStream)> {
        let race = !self.policy.read().unwrap().sticky;
        let mut tried: Vec<String> = Vec::new();
        let mut dials = JoinSet::new();
        let mut last = no_network();
        loop {
            let next = self.choose(key, &tried, pin);
            let more = next.is_some();
            let mut wait = RACE_AFTER_MAX;
            if let Some(link) = next {
                if let Some(ms) = link.latency_ms() {
                    wait = Duration::from_millis(u64::from(ms) * 4).clamp(RACE_AFTER_MIN, RACE_AFTER_MAX);
                }
                tried.push(link.id.clone());
                let info = link.info();
                let lease = Lease::open(link, false);
                dials.spawn(async move {
                    let dialed = dial::tcp(&info, to, CONNECT_LIMIT).await;
                    (lease, dialed)
                });
            }
            if dials.is_empty() {
                return Err(last);
            }
            let next_round = tokio::time::sleep(wait);
            tokio::pin!(next_round);
            loop {
                tokio::select! {
                    done = dials.join_next() => match done {
                        Some(Ok((lease, Ok(stream)))) => return Ok((lease, stream)),
                        Some(Ok((lease, Err(err)))) => {
                            log::debug!("{to} through {}: {err}", lease.link.name());
                            last = err;
                            if more || dials.is_empty() {
                                break;
                            }
                        }
                        Some(Err(_)) if !dials.is_empty() => {}
                        Some(Err(_)) | None => break,
                    },
                    _ = &mut next_round, if more && race => break,
                }
            }
        }
    }

    pub fn bind_udp(&self, key: Option<&str>, pin: LinkPin) -> io::Result<(Lease, UdpSocket)> {
        let mut tried: Vec<String> = Vec::new();
        let mut last = no_network();
        for _ in 0..2 {
            let Some(link) = self.choose(key, &tried, pin) else {
                break;
            };
            match dial::udp(&link.info()) {
                Ok(socket) => return Ok((Lease::open(link, true), socket)),
                Err(err) => {
                    tried.push(link.id.clone());
                    last = err;
                }
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::link::{LinkInfo, Reach};
    use std::net::Ipv4Addr;

    fn link(id: &str) -> Arc<Link> {
        Arc::new(Link::new(
            id.into(),
            LinkInfo {
                name: id.into(),
                ipv4: Some(Ipv4Addr::new(192, 168, 1, 2)),
                present: true,
                ..LinkInfo::default()
            },
        ))
    }

    fn picker(links: &[Arc<Link>], policy: Policy) -> Picker {
        Picker::new(Arc::new(LinkSet::with(links.to_vec())), policy)
    }

    fn set(policy: &mut Policy, id: &str, change: impl FnOnce(&mut LinkPolicy)) {
        let entry = policy.links.entry(id.into()).or_insert(LinkPolicy {
            enabled: true,
            weight: 5,
            backup: false,
        });
        change(entry);
    }

    fn spread(p: &Picker, n: usize) -> HashMap<String, usize> {
        let mut leases = Vec::new();
        let mut count = HashMap::new();
        for _ in 0..n {
            let l = p.choose(None, &[], None).unwrap();
            *count.entry(l.id.clone()).or_default() += 1;
            leases.push(Lease::open(l, false));
        }
        count
    }

    #[test]
    fn weights_set_the_split() {
        let mut policy = Policy::default();
        set(&mut policy, "a", |p| p.weight = 6);
        set(&mut policy, "b", |p| p.weight = 3);
        let count = spread(&picker(&[link("a"), link("b")], policy), 9);
        assert_eq!(count["a"], 6);
        assert_eq!(count["b"], 3);
    }

    #[test]
    fn backups_wait_until_the_rest_is_down() {
        let mut policy = Policy::default();
        set(&mut policy, "phone", |p| p.backup = true);
        let wifi = link("wifi");
        let p = picker(&[wifi.clone(), link("phone")], policy);
        assert_eq!(spread(&p, 4)["wifi"], 4);
        wifi.missed();
        wifi.missed();
        assert_eq!(p.choose(None, &[], None).unwrap().id, "phone");
    }

    #[test]
    fn full_reach_beats_in_country_only() {
        let local = link("local");
        local.heard(Reach::Local, Some(10));
        let p = picker(&[local, link("full")], Policy::default());
        assert_eq!(spread(&p, 3)["full"], 3);
    }

    #[test]
    fn a_down_network_is_still_used_when_it_is_the_only_one() {
        let only = link("only");
        only.missed();
        only.missed();
        let p = picker(&[only], Policy::default());
        assert_eq!(p.choose(None, &[], None).unwrap().id, "only");
    }

    #[test]
    fn capped_and_switched_off_networks_are_skipped() {
        let capped = link("capped");
        capped.set_capped(true);
        let mut policy = Policy::default();
        set(&mut policy, "off", |p| p.enabled = false);
        let p = picker(&[capped, link("off"), link("on")], policy);
        assert_eq!(spread(&p, 3)["on"], 3);
    }

    #[test]
    fn virtual_adapters_start_switched_off() {
        let vpn = Arc::new(Link::new(
            "vpn".into(),
            LinkInfo {
                ipv4: Some(Ipv4Addr::new(10, 8, 0, 2)),
                present: true,
                virtual_adapter: true,
                ..LinkInfo::default()
            },
        ));
        let p = picker(&[vpn], Policy::default());
        assert!(p.choose(None, &[], None).is_none());
    }

    #[test]
    fn pins_hold_unless_they_may_fall_back() {
        let p = picker(&[link("a"), link("b")], Policy::default());
        let id = |l: Option<Arc<Link>>| l.map(|l| l.id.clone());
        assert_eq!(id(p.choose(None, &[], Some(("b", false)))).as_deref(), Some("b"));
        assert_eq!(id(p.choose(None, &[], Some(("gone", false)))), None);
        assert!(p.choose(None, &[], Some(("gone", true))).is_some());
    }

    #[test]
    fn sticky_sites_stay_put() {
        let policy = Policy {
            sticky: true,
            ..Policy::default()
        };
        let p = picker(&[link("a"), link("b")], policy);
        let first = p.choose(Some("bank.example"), &[], None).unwrap();
        let _busy = Lease::open(first.clone(), false);
        for _ in 0..3 {
            assert_eq!(p.choose(Some("bank.example"), &[], None).unwrap().id, first.id);
        }
        assert_ne!(p.choose(None, &[], None).unwrap().id, first.id);
    }
}
