use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ipstack::IpStackUdpStream;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio::time::Instant;

use super::flows::Flow;
use super::{Ctx, MTU};
use crate::net::dial;
use crate::net::pick::Lease;
use crate::win;

pub const PORT: u16 = 53;
/// Still no answer after this long: ask each network's own resolvers too.
const WIDEN_AFTER: Duration = Duration::from_millis(700);
/// A "server failure" or "refused" waits this long for a better answer.
const HOLD_FAILURE: Duration = Duration::from_millis(400);
const GIVE_UP: Duration = Duration::from_secs(6);

struct Path {
    lease: Lease,
    socket: Arc<UdpSocket>,
    own: Vec<SocketAddr>,
}

struct Question {
    bytes: Vec<u8>,
    asked: Instant,
    widened: bool,
    held: Option<(Instant, usize, Vec<u8>)>,
}

impl Question {
    fn next_step(&self) -> Instant {
        let mut at = self.asked + GIVE_UP;
        if !self.widened {
            at = at.min(self.asked + WIDEN_AFTER);
        }
        if let Some((held, ..)) = &self.held {
            at = at.min(*held + HOLD_FAILURE);
        }
        at
    }
}

/// Sends each lookup through every network at once and hands the app the
/// first good answer, so one slow or lossy network never holds a page up.
pub async fn serve(app: IpStackUdpStream, ctx: Arc<Ctx>, flow: Arc<Flow>) {
    let server = flow.dst;
    let mut paths = Vec::new();
    for link in ctx.picker.spread() {
        let info = link.info();
        let Ok(socket) = dial::udp(&info) else { continue };
        let own = info
            .dns
            .iter()
            .map(|ip| SocketAddr::new(*ip, PORT))
            .filter(|addr| *addr != server)
            .collect();
        paths.push(Path {
            lease: Lease::open(link, true),
            socket: Arc::new(socket),
            own,
        });
    }
    if paths.is_empty() {
        return;
    }

    let (tx, mut answers) = mpsc::channel::<(usize, Vec<u8>)>(32);
    let mut listeners = JoinSet::new();
    for (i, path) in paths.iter().enumerate() {
        let mut from = vec![server];
        from.extend(path.own.iter().copied());
        listeners.spawn(listen(i, path.socket.clone(), from, tx.clone()));
    }
    drop(tx);

    let (mut app_r, mut app_w) = tokio::io::split(app);
    let mut buf = vec![0u8; usize::from(MTU)];
    let mut open: HashMap<u16, Question> = HashMap::new();
    let mut idle_until = Instant::now() + GIVE_UP;
    loop {
        let wake = open.values().map(Question::next_step).min().unwrap_or(idle_until);
        tokio::select! {
            biased;
            read = app_r.read(&mut buf) => {
                let Ok(n @ 2..) = read else { break };
                let question = buf[..n].to_vec();
                for path in &paths {
                    send(path, &question, &[server]).await;
                }
                flow.sent(n);
                open.insert(u16::from_be_bytes([question[0], question[1]]), Question {
                    bytes: question,
                    asked: Instant::now(),
                    widened: false,
                    held: None,
                });
                idle_until = Instant::now() + GIVE_UP;
            }
            got = answers.recv() => {
                let Some((i, answer)) = got else { break };
                if answer.len() < 4 {
                    continue;
                }
                let id = u16::from_be_bytes([answer[0], answer[1]]);
                let Some(question) = open.get_mut(&id) else { continue };
                let code = answer[3] & 0x0f;
                if code == 2 || code == 5 {
                    question.held.get_or_insert((Instant::now(), i, answer));
                    continue;
                }
                open.remove(&id);
                if !deliver(&mut app_w, &paths[i], &flow, &answer).await {
                    break;
                }
                idle_until = Instant::now() + GIVE_UP;
            }
            _ = tokio::time::sleep_until(wake) => {
                let now = Instant::now();
                if open.is_empty() && now >= idle_until {
                    break;
                }
                let mut finished = Vec::new();
                for (id, question) in open.iter_mut() {
                    if !question.widened && now >= question.asked + WIDEN_AFTER {
                        question.widened = true;
                        for path in &paths {
                            send(path, &question.bytes, &path.own).await;
                        }
                    }
                    let held_long = question.held.as_ref().is_some_and(|(at, ..)| now >= *at + HOLD_FAILURE);
                    if held_long || now >= question.asked + GIVE_UP {
                        finished.push(*id);
                    }
                }
                for id in finished {
                    let Some(Question { held: Some((_, i, answer)), .. }) = open.remove(&id) else { continue };
                    if !deliver(&mut app_w, &paths[i], &flow, &answer).await {
                        return;
                    }
                }
            }
        }
    }
}

async fn send(path: &Path, question: &[u8], to: &[SocketAddr]) {
    for addr in to {
        if path.socket.send_to(question, addr).await.is_ok() {
            path.lease.sent(question.len());
        }
    }
}

async fn listen(path: usize, socket: Arc<UdpSocket>, from: Vec<SocketAddr>, out: mpsc::Sender<(usize, Vec<u8>)>) {
    let mut buf = vec![0u8; usize::from(MTU)];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((n, sender)) if from.contains(&sender) => {
                if out.send((path, buf[..n].to_vec())).await.is_err() {
                    return;
                }
            }
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::ConnectionReset || win::is_msg_size(&err) => {}
            Err(_) => return,
        }
    }
}

async fn deliver<W: AsyncWrite + Unpin>(app: &mut W, path: &Path, flow: &Flow, answer: &[u8]) -> bool {
    if app.write_all(answer).await.is_err() {
        return false;
    }
    path.lease.received(answer.len());
    flow.received(answer.len());
    let _ = flow.link.set(path.lease.link.id.clone());
    true
}
