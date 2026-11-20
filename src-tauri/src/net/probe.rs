use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::dial;
use super::link::{Link, LinkInfo, LinkSet, Reach};

const EVERY: Duration = Duration::from_secs(4);
const ANSWER_WITHIN: Duration = Duration::from_millis(2500);
/// A network that carried this much since the last round is working.
const CARRIED: u64 = 32 * 1024;

/// Anycast HTTPS front ends of four operators, tried side by side; a TCP
/// handshake proves the path without sending anything.
const WORLD: [&str; 4] = ["1.1.1.1:443", "8.8.8.8:443", "9.9.9.9:443", "208.67.222.222:443"];
/// Resolvers hosted in Iran, which still answer while the country is cut
/// off from the rest of the internet.
const IN_COUNTRY: [&str; 3] = ["178.22.122.100:53", "78.157.42.100:53", "185.55.226.26:53"];

async fn first_answer(link: &LinkInfo, targets: &[&str]) -> Option<u32> {
    let start = Instant::now();
    let mut dials = JoinSet::new();
    for target in targets {
        let to: SocketAddr = target.parse().expect("probe targets are valid");
        let link = link.clone();
        dials.spawn(async move { dial::tcp(&link, to, ANSWER_WITHIN).await.is_ok() });
    }
    while let Some(done) = dials.join_next().await {
        if done.unwrap_or(false) {
            return Some(start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32);
        }
    }
    None
}

async fn round(link: &Link) -> bool {
    let info = link.info();
    if let Some(ms) = first_answer(&info, &WORLD).await {
        link.heard(Reach::Full, Some(ms));
        return true;
    }
    if let Some(ms) = first_answer(&info, &IN_COUNTRY).await {
        link.heard(Reach::Local, Some(ms));
        return true;
    }
    false
}

async fn check(link: Arc<Link>) {
    let carried = link.rx_since_probe() >= CARRIED;
    if round(&link).await {
        return;
    }
    if carried {
        let reach = link.reach();
        link.heard(if reach == Reach::Unknown { Reach::Full } else { reach }, None);
        return;
    }
    // One silent round can be a lost packet; ask again at once.
    if !link.missed() && !round(&link).await {
        link.missed();
    }
}

pub async fn run(links: Arc<LinkSet>, stop: CancellationToken) {
    loop {
        let mut checks = JoinSet::new();
        for link in links.all().into_iter().filter(|l| l.present()) {
            checks.spawn(check(link));
        }
        while checks.join_next().await.is_some() {}
        tokio::select! {
            _ = tokio::time::sleep(EVERY) => {}
            _ = stop.cancelled() => return,
        }
    }
}
