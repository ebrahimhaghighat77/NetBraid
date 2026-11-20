mod dns;
mod flows;
mod icmp;
mod owners;
mod sniff;
mod tcp;
mod udp;

use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ipstack::{IpStack, IpStackConfig, IpStackStream, TcpConfig};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tun::AbstractDevice;

pub use flows::FlowTable;

use crate::net::link::LinkSet;
use flows::Flow;
use crate::net::pick::Picker;
use crate::win::{self, CaptureRoute, Ipv4First};
use owners::Owners;

pub const TUN_NAME: &str = "NetBraid";
pub const TUN_ADDR: Ipv4Addr = Ipv4Addr::new(10, 222, 0, 1);
pub const MTU: u16 = 1500;
/// A fixed adapter GUID, so Windows keeps one network profile for it.
const TUN_GUID: u128 = 0x4e42_7261_6964_4e42_9d31_6c5b_2e80_a7c4;
/// Two halves of the address space: more specific than any default route,
/// so they win without touching the real ones.
const CAPTURE: [Ipv4Addr; 2] = [Ipv4Addr::new(0, 0, 0, 0), Ipv4Addr::new(128, 0, 0, 0)];
/// On a failing network, a flow that received nothing for this long is
/// closed so its app reconnects elsewhere.
const STALL: Duration = Duration::from_secs(4);

pub struct Ctx {
    pub picker: Arc<Picker>,
    pub flows: Arc<FlowTable>,
    owners: Owners,
}

impl Ctx {
    /// The app behind a flow, looked up only while some rule names an app.
    pub async fn app_of(self: &Arc<Self>, flow: &Arc<Flow>) -> Option<String> {
        if let Some(app) = flow.app.get() {
            return app.clone();
        }
        if !self.picker.has_app_rules() {
            return None;
        }
        let (ctx, f) = (self.clone(), flow.clone());
        let app = tokio::task::spawn_blocking(move || ctx.owners.app(f.udp, f.src.port(), f.opened))
            .await
            .ok()
            .flatten();
        let _ = flow.app.set(app.clone());
        app
    }
}

pub struct Engine {
    pub flows: Arc<FlowTable>,
    routes: Vec<CaptureRoute>,
    ipv4_first: Option<Ipv4First>,
    stop: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}

impl Engine {
    /// Creates the adapter and starts sending everything through it.
    pub async fn start(
        wintun: PathBuf,
        dns: Vec<IpAddr>,
        picker: Arc<Picker>,
        links: Arc<LinkSet>,
    ) -> io::Result<Engine> {
        if !wintun.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is missing", wintun.display()),
            ));
        }
        clean_stale();
        let device = tokio::task::spawn_blocking(move || open_device(wintun, dns))
            .await
            .map_err(io::Error::other)??;
        let index = device.tun_index().map_err(io::Error::other)? as u32;
        let routes = CAPTURE
            .iter()
            .map(|prefix| CaptureRoute::add(index, *prefix, 1))
            .collect::<io::Result<Vec<_>>>()?;
        let device = tun::AsyncDevice::new(device)?;
        let ipv4_first = Ipv4First::apply()
            .inspect_err(|err| log::warn!("could not make Windows prefer IPv4: {err}"))
            .ok();

        let flows = Arc::new(FlowTable::default());
        let ctx = Arc::new(Ctx {
            picker,
            flows: flows.clone(),
            owners: Owners::default(),
        });
        let stop = CancellationToken::new();
        let stack = IpStack::new(stack_config(), device);
        let tasks = vec![
            tokio::spawn(accept(stack, ctx.clone(), stop.clone())),
            tokio::spawn(watch(links, ctx, stop.clone())),
        ];
        log::info!("turned on (adapter {index})");
        Ok(Engine {
            flows,
            routes,
            ipv4_first,
            stop,
            tasks,
        })
    }

    pub fn running(&self) -> bool {
        self.tasks.iter().all(|t| !t.is_finished())
    }

    pub async fn stop(self) {
        // Routes first, so traffic goes straight back to the real networks.
        drop(self.routes);
        drop(self.ipv4_first);
        self.stop.cancel();
        for task in self.tasks {
            let _ = task.await;
        }
        log::info!("turned off");
    }
}

fn open_device(wintun: PathBuf, dns: Vec<IpAddr>) -> io::Result<tun::Device> {
    let mut config = tun::Configuration::default();
    config
        .tun_name(TUN_NAME)
        .address(TUN_ADDR)
        .netmask(Ipv4Addr::new(255, 255, 255, 0))
        .mtu(MTU)
        .metric(1)
        .up();
    config.platform_config(move |p| {
        p.wintun_file(wintun.into_os_string());
        p.device_guid(TUN_GUID);
        if !dns.is_empty() {
            p.dns_servers(&dns);
        }
    });
    tun::create(&config).map_err(io::Error::other)
}

/// Removes routes a crashed run left on the adapter.
fn clean_stale() {
    for adapter in win::list_adapters().into_iter().filter(|a| a.name == TUN_NAME) {
        for prefix in CAPTURE {
            if win::delete_route(adapter.index, prefix, 1) {
                log::info!("removed a leftover route {prefix}/1");
            }
        }
    }
}

fn stack_config() -> IpStackConfig {
    let mut tcp = TcpConfig::default();
    tcp.timeout = Duration::from_secs(600);
    tcp.max_unacked_bytes = 256 * 1024;
    tcp.read_buffer_size = 256 * 1024;
    let mut config = IpStackConfig::default();
    config
        .mtu_unchecked(MTU)
        .udp_timeout(Duration::from_secs(60))
        .with_tcp_config(tcp);
    config
}

async fn accept(mut stack: IpStack, ctx: Arc<Ctx>, stop: CancellationToken) {
    loop {
        let next = tokio::select! {
            next = stack.accept() => next,
            _ = stop.cancelled() => break,
        };
        match next {
            Ok(IpStackStream::Tcp(stream)) => {
                let flow = ctx.flows.open(false, stream.local_addr(), stream.peer_addr());
                spawn_flow(&ctx, flow.clone(), tcp::serve(stream, ctx.clone(), flow));
            }
            Ok(IpStackStream::Udp(stream)) => {
                let flow = ctx.flows.open(true, stream.local_addr(), stream.peer_addr());
                spawn_flow(&ctx, flow.clone(), udp::serve(stream, ctx.clone(), flow));
            }
            Ok(IpStackStream::UnknownTransport(packet)) => {
                tokio::spawn(icmp::answer(packet, ctx.clone()));
            }
            Ok(IpStackStream::UnknownNetwork(_)) => {}
            Err(err) => {
                log::error!("the packet stack stopped: {err}");
                break;
            }
        }
    }
    ctx.flows.end_all();
}

struct Unlist(Arc<FlowTable>, u64);

impl Drop for Unlist {
    fn drop(&mut self) {
        self.0.remove(self.1);
    }
}

fn spawn_flow(ctx: &Arc<Ctx>, flow: Arc<Flow>, relay: impl Future<Output = ()> + Send + 'static) {
    let unlist = Unlist(ctx.flows.clone(), flow.id);
    let task = tokio::spawn(async move {
        let _unlist = unlist;
        relay.await;
    });
    ctx.flows.set_abort(flow.id, task.abort_handle());
}

/// Closes flows on networks that went away, and stalled flows on failing
/// ones, so apps reconnect through a network that works.
async fn watch(links: Arc<LinkSet>, ctx: Arc<Ctx>, stop: CancellationToken) {
    let mut every = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = every.tick() => {}
            _ = stop.cancelled() => return,
        }
        let all = links.all();
        let elsewhere = all.iter().any(|l| l.usable() && ctx.picker.enabled(l));
        for link in &all {
            if !link.present() || link.capped() {
                let ended = ctx.flows.end_link(&link.id);
                if ended > 0 {
                    log::info!("{}: closed {ended} connections, the network is unavailable", link.name());
                }
            } else if link.down() && elsewhere {
                for flow in ctx.flows.on_link(&link.id) {
                    if flow.quiet_for() >= STALL && flow.end() {
                        log::debug!("{}: closed a stalled connection to {}", link.name(), flow.dst);
                    }
                }
            }
        }
    }
}
