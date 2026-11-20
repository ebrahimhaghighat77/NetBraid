use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use ipstack::IpStackUnknownTransport;

use super::Ctx;
use crate::win;

const ICMP: u8 = 1;
const ECHO_REQUEST: u8 = 8;
const ECHO_REPLY: u8 = 0;
const WAIT: Duration = Duration::from_secs(2);

/// Answers a ping by sending a real one through a network, so `ping`
/// keeps working while everything goes through the adapter.
pub async fn answer(packet: IpStackUnknownTransport, ctx: Arc<Ctx>) {
    if packet.ip_protocol().0 != ICMP {
        return;
    }
    let IpAddr::V4(to) = packet.dst_addr() else { return };
    let request = packet.payload().to_vec();
    if request.len() < 8 || request[0] != ECHO_REQUEST {
        return;
    }
    let Some(link) = ctx.picker.choose(Some(&to.to_string()), &[], None) else { return };
    let Some(from) = link.ipv4() else { return };
    let data = request[8..].to_vec();
    let answered = tokio::task::spawn_blocking(move || win::echo_v4(from, to, &data, 64, WAIT))
        .await
        .ok()
        .flatten();
    if answered.is_none() {
        return;
    }
    let mut reply = request;
    reply[0] = ECHO_REPLY;
    reply[2] = 0;
    reply[3] = 0;
    let sum = checksum(&reply);
    reply[2..4].copy_from_slice(&sum.to_be_bytes());
    let _ = packet.send(reply);
}

fn checksum(bytes: &[u8]) -> u16 {
    let mut sum: u32 = bytes
        .chunks(2)
        .map(|pair| u32::from(u16::from_be_bytes([pair[0], pair.get(1).copied().unwrap_or(0)])))
        .sum();
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_of_a_packet_with_its_checksum_is_zero() {
        let mut reply = vec![0, 0, 0, 0, 0x12, 0x34, 0x00, 0x01, b'a', b'b', b'c'];
        let sum = checksum(&reply);
        reply[2..4].copy_from_slice(&sum.to_be_bytes());
        assert_eq!(checksum(&reply), 0);
    }
}
