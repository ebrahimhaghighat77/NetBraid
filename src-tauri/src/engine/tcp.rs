use std::io;
use std::sync::Arc;
use std::time::Duration;

use ipstack::IpStackTcpStream;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

use super::flows::Flow;
use super::sniff::{self, Sniffed};
use super::Ctx;
use crate::net::pick::Lease;
use crate::net::rules::Verdict;

/// Ports where the client speaks first and names the site.
const NAMED_PORTS: [u16; 4] = [80, 443, 8080, 8443];
const NAME_WAIT: Duration = Duration::from_millis(350);
/// After one side has finished, how long the other may keep going alone.
const LINGER: Duration = Duration::from_secs(20);
const CLOSE_WAIT: Duration = Duration::from_secs(2);
const BUF_START: usize = 4 * 1024;
const BUF_MAX: usize = 64 * 1024;

pub async fn serve(mut app: IpStackTcpStream, ctx: Arc<Ctx>, flow: Arc<Flow>) {
    let dst = flow.dst;
    if !dst.is_ipv4() {
        return;
    }
    let first = if NAMED_PORTS.contains(&dst.port()) {
        match first_bytes(&mut app, &flow).await {
            Some(first) => first,
            None => return,
        }
    } else {
        Vec::new()
    };

    let app_name = ctx.app_of(&flow).await;
    let host = flow.host.get().cloned();
    let pin = match ctx.picker.verdict(app_name.as_deref(), host.as_deref(), dst.ip()) {
        Verdict::Block => {
            let _ = tokio::time::timeout(CLOSE_WAIT, app.shutdown()).await;
            return;
        }
        Verdict::Only { link, fallback } => Some((link, fallback)),
        Verdict::Any => None,
    };
    let pin = pin.as_ref().map(|(link, fallback)| (link.as_str(), *fallback));
    let key = host.as_deref().map(sniff::site).map(str::to_owned).unwrap_or_else(|| dst.ip().to_string());

    let (lease, site) = match ctx.picker.connect_tcp(dst, Some(&key), pin).await {
        Ok(connected) => connected,
        Err(err) => {
            log::debug!("{key} ({dst}): {err}");
            return;
        }
    };
    let _ = flow.link.set(lease.link.id.clone());
    relay(app, site, first, &flow, &lease).await;
}

/// What the client sends first, read until the site name is known or it's
/// clear there is none. `None` when the client went away.
async fn first_bytes(app: &mut IpStackTcpStream, flow: &Flow) -> Option<Vec<u8>> {
    let deadline = tokio::time::Instant::now() + NAME_WAIT;
    let mut got = Vec::new();
    let mut buf = [0u8; 2048];
    while got.len() < sniff::LIMIT {
        match tokio::time::timeout_at(deadline, app.read(&mut buf)).await {
            Err(_) => break,
            Ok(Ok(0)) | Ok(Err(_)) => return None,
            Ok(Ok(n)) => got.extend_from_slice(&buf[..n]),
        }
        match sniff::host(&got) {
            Sniffed::Found(name) => {
                let _ = flow.host.set(name);
                break;
            }
            Sniffed::Nothing => break,
            Sniffed::NeedMore => {}
        }
    }
    Some(got)
}

async fn relay(app: IpStackTcpStream, site: TcpStream, first: Vec<u8>, flow: &Flow, lease: &Lease) {
    let (mut app_r, mut app_w) = tokio::io::split(app);
    let (mut site_r, mut site_w) = site.into_split();
    if !first.is_empty() {
        if site_w.write_all(&first).await.is_err() {
            return;
        }
        flow.sent(first.len());
        lease.sent(first.len());
    }
    let (up_done, down_done) = (CancellationToken::new(), CancellationToken::new());
    let up = async {
        tokio::select! {
            _ = pump(&mut app_r, &mut site_w, |n| { flow.sent(n); lease.sent(n) }) => {}
            _ = flow.cancel.cancelled() => {}
            _ = linger(&down_done) => {}
        }
        up_done.cancel();
        let _ = site_w.shutdown().await;
    };
    let down = async {
        tokio::select! {
            _ = pump(&mut site_r, &mut app_w, |n| { flow.received(n); lease.received(n) }) => {}
            _ = flow.cancel.cancelled() => {}
            _ = linger(&up_done) => {}
        }
        down_done.cancel();
        // A close is what makes the app reconnect, through another network if need be.
        let _ = tokio::time::timeout(CLOSE_WAIT, app_w.shutdown()).await;
    };
    tokio::join!(up, down);
}

async fn linger(other: &CancellationToken) {
    other.cancelled().await;
    tokio::time::sleep(LINGER).await;
}

async fn pump<R, W>(from: &mut R, to: &mut W, count: impl Fn(usize)) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; BUF_START];
    loop {
        let n = from.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }
        to.write_all(&buf[..n]).await?;
        count(n);
        if n == buf.len() && buf.len() < BUF_MAX {
            buf.resize(buf.len() * 2, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn pump_copies_everything_and_counts_it() {
        let (mut from, mut far) = tokio::io::duplex(1024);
        let data: Vec<u8> = (0..300_000u32).map(|i| i as u8).collect();
        let writer = async {
            far.write_all(&data).await.unwrap();
            far.shutdown().await.unwrap();
        };
        let mut out = Vec::new();
        let total = AtomicUsize::new(0);
        let copier = pump(&mut from, &mut out, |n| {
            total.fetch_add(n, Ordering::Relaxed);
        });
        let (_, copied) = tokio::join!(writer, copier);
        copied.unwrap();
        assert_eq!(out, data);
        assert_eq!(total.load(Ordering::Relaxed), data.len());
    }
}
