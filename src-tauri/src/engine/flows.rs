use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::task::AbortHandle;
use tokio_util::sync::CancellationToken;

/// A flow that received data this recently, or opened this recently, is busy.
const BUSY_WITHIN: Duration = Duration::from_secs(2);
const NEW_FOR: Duration = Duration::from_secs(3);

pub struct Flow {
    pub id: u64,
    pub udp: bool,
    pub src: SocketAddr,
    pub dst: SocketAddr,
    pub opened: Instant,
    pub host: OnceLock<String>,
    pub app: OnceLock<Option<String>>,
    pub link: OnceLock<String>,
    pub rx: AtomicU64,
    pub tx: AtomicU64,
    last_rx_ms: AtomicU64,
    epoch: Instant,
    pub cancel: CancellationToken,
    abort: Mutex<Option<AbortHandle>>,
}

impl Flow {
    pub fn received(&self, n: usize) {
        self.rx.fetch_add(n as u64, Relaxed);
        self.last_rx_ms.store(self.epoch.elapsed().as_millis() as u64, Relaxed);
    }

    pub fn sent(&self, n: usize) {
        self.tx.fetch_add(n as u64, Relaxed);
    }

    pub fn quiet_for(&self) -> Duration {
        let last = Duration::from_millis(self.last_rx_ms.load(Relaxed));
        self.epoch.elapsed().saturating_sub(last)
    }

    fn busy(&self) -> bool {
        self.opened.elapsed() < NEW_FOR || self.quiet_for() < BUSY_WITHIN
    }

    /// Asks the flow to end: a TCP relay closes the app's side so it
    /// reconnects, a UDP relay just stops. False if it was already asked.
    pub fn end(&self) -> bool {
        if self.cancel.is_cancelled() {
            return false;
        }
        self.cancel.cancel();
        if self.udp {
            if let Some(handle) = self.abort.lock().unwrap().as_ref() {
                handle.abort();
            }
        }
        true
    }
}

pub struct FlowTable {
    epoch: Instant,
    next: AtomicU64,
    map: Mutex<HashMap<u64, Arc<Flow>>>,
}

impl Default for FlowTable {
    fn default() -> Self {
        FlowTable {
            epoch: Instant::now(),
            next: AtomicU64::new(1),
            map: Mutex::new(HashMap::new()),
        }
    }
}

impl FlowTable {
    pub fn open(&self, udp: bool, src: SocketAddr, dst: SocketAddr) -> Arc<Flow> {
        let flow = Arc::new(Flow {
            id: self.next.fetch_add(1, Relaxed),
            udp,
            src,
            dst,
            opened: Instant::now(),
            host: OnceLock::new(),
            app: OnceLock::new(),
            link: OnceLock::new(),
            rx: AtomicU64::new(0),
            tx: AtomicU64::new(0),
            last_rx_ms: AtomicU64::new(self.epoch.elapsed().as_millis() as u64),
            epoch: self.epoch,
            cancel: CancellationToken::new(),
            abort: Mutex::new(None),
        });
        self.map.lock().unwrap().insert(flow.id, flow.clone());
        flow
    }

    pub fn set_abort(&self, id: u64, handle: AbortHandle) {
        if let Some(flow) = self.map.lock().unwrap().get(&id) {
            *flow.abort.lock().unwrap() = Some(handle);
        }
    }

    pub fn remove(&self, id: u64) {
        self.map.lock().unwrap().remove(&id);
    }

    pub fn list(&self) -> Vec<Arc<Flow>> {
        self.map.lock().unwrap().values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap().len()
    }

    pub fn on_link(&self, link_id: &str) -> Vec<Arc<Flow>> {
        self.list()
            .into_iter()
            .filter(|f| f.link.get().map(String::as_str) == Some(link_id))
            .collect()
    }

    /// Ends every flow on a network; returns how many were still running.
    pub fn end_link(&self, link_id: &str) -> usize {
        self.on_link(link_id).iter().filter(|f| f.end()).count()
    }

    pub fn end_all(&self) {
        for flow in self.list() {
            flow.end();
            if let Some(handle) = flow.abort.lock().unwrap().as_ref() {
                handle.abort();
            }
        }
    }

    pub fn busy_by_link(&self) -> HashMap<String, u32> {
        let mut busy = HashMap::new();
        for flow in self.list() {
            if let Some(link) = flow.link.get() {
                if flow.busy() {
                    *busy.entry(link.clone()).or_default() += 1;
                }
            }
        }
        busy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(port: u16) -> SocketAddr {
        SocketAddr::from(([10, 222, 0, 1], port))
    }

    #[test]
    fn ending_a_network_touches_only_its_flows_once() {
        let table = FlowTable::default();
        let a = table.open(false, addr(1), addr(443));
        let b = table.open(false, addr(2), addr(443));
        a.link.set("wifi".into()).unwrap();
        b.link.set("phone".into()).unwrap();
        assert_eq!(table.end_link("wifi"), 1);
        assert_eq!(table.end_link("wifi"), 0);
        assert!(a.cancel.is_cancelled() && !b.cancel.is_cancelled());
    }

    #[test]
    fn new_flows_count_as_busy() {
        let table = FlowTable::default();
        let flow = table.open(true, addr(1), addr(53));
        flow.link.set("wifi".into()).unwrap();
        table.open(true, addr(2), addr(53));
        assert_eq!(table.busy_by_link().get("wifi"), Some(&1));
        assert_eq!(table.len(), 2);
    }
}
