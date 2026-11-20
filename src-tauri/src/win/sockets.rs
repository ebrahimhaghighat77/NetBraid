use std::io;
use std::os::windows::io::AsRawSocket;

use socket2::Socket;
use windows_sys::Win32::Networking::WinSock::{setsockopt, IPPROTO_IP, IP_UNICAST_IF, WSAEMSGSIZE};

/// Makes the socket leave through one interface whatever the routing table
/// says, so relayed traffic never loops back into the capture adapter.
pub fn bind_to_interface(socket: &Socket, if_index: u32) -> io::Result<()> {
    // IP_UNICAST_IF wants the index in network byte order.
    let value = if_index.to_be();
    let rc = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            IPPROTO_IP,
            IP_UNICAST_IF,
            (&value as *const u32).cast(),
            std::mem::size_of::<u32>() as i32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// A datagram was larger than the receive buffer; the socket itself is fine.
pub fn is_msg_size(err: &io::Error) -> bool {
    err.raw_os_error() == Some(WSAEMSGSIZE)
}
