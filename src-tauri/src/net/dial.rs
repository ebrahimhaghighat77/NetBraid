use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::{TcpSocket, TcpStream, UdpSocket};

use super::link::LinkInfo;
use crate::win;

fn pinned(link: &LinkInfo, kind: Type, protocol: Protocol) -> io::Result<Socket> {
    let source = link.ipv4.ok_or_else(|| {
        io::Error::new(io::ErrorKind::AddrNotAvailable, format!("{} has no IPv4 address", link.name))
    })?;
    let socket = Socket::new(Domain::IPV4, kind, Some(protocol))?;
    win::bind_to_interface(&socket, link.index)?;
    socket.bind(&SockAddr::from(SocketAddr::new(IpAddr::V4(source), 0)))?;
    socket.set_nonblocking(true)?;
    Ok(socket)
}

pub async fn tcp(link: &LinkInfo, to: SocketAddr, limit: Duration) -> io::Result<TcpStream> {
    let socket = pinned(link, Type::STREAM, Protocol::TCP)?;
    socket.set_tcp_nodelay(true)?;
    let socket = TcpSocket::from_std_stream(socket.into());
    match tokio::time::timeout(limit, socket.connect(to)).await {
        Ok(connected) => connected,
        Err(_) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            format!("{to} did not answer through {}", link.name),
        )),
    }
}

pub fn udp(link: &LinkInfo) -> io::Result<UdpSocket> {
    let socket = pinned(link, Type::DGRAM, Protocol::UDP)?;
    UdpSocket::from_std(socket.into())
}
