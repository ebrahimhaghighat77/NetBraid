use std::io;
use std::sync::Arc;

use ipstack::IpStackUdpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::flows::Flow;
use super::{dns, Ctx, MTU};
use crate::net::rules::Verdict;
use crate::win;

pub async fn serve(app: IpStackUdpStream, ctx: Arc<Ctx>, flow: Arc<Flow>) {
    let dst = flow.dst;
    if !dst.is_ipv4() {
        return;
    }
    let app_name = ctx.app_of(&flow).await;
    let pin = match ctx.picker.verdict(app_name.as_deref(), None, dst.ip()) {
        Verdict::Block => return,
        Verdict::Only { link, fallback } => Some((link, fallback)),
        Verdict::Any => None,
    };
    if dst.port() == dns::PORT && pin.is_none() {
        return dns::serve(app, ctx, flow).await;
    }
    let pin = pin.as_ref().map(|(link, fallback)| (link.as_str(), *fallback));
    // QUIC hides the site name, so a destination is the closest thing to a site.
    let key = dst.ip().to_string();
    let (lease, socket) = match ctx.picker.bind_udp(Some(&key), pin) {
        Ok(bound) => bound,
        Err(err) => {
            log::debug!("udp {dst}: {err}");
            return;
        }
    };
    if socket.connect(dst).await.is_err() {
        return;
    }
    let _ = flow.link.set(lease.link.id.clone());

    let (mut app_r, mut app_w) = tokio::io::split(app);
    let mut up = vec![0u8; usize::from(MTU)];
    let mut down = vec![0u8; usize::from(MTU)];
    loop {
        tokio::select! {
            biased;
            read = app_r.read(&mut up) => {
                // The stack ends the stream once the flow has been idle.
                let Ok(n @ 1..) = read else { break };
                if socket.send(&up[..n]).await.is_ok() {
                    lease.sent(n);
                    flow.sent(n);
                }
            }
            got = socket.recv(&mut down) => match got {
                Ok(n) => {
                    if app_w.write_all(&down[..n]).await.is_err() {
                        break;
                    }
                    lease.received(n);
                    flow.received(n);
                }
                Err(err) if err.kind() == io::ErrorKind::ConnectionReset || win::is_msg_size(&err) => {}
                Err(_) => break,
            },
        }
    }
}
