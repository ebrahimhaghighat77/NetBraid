use std::collections::HashMap;
use std::net::Ipv4Addr;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCPROW_OWNER_PID, MIB_UDPROW_OWNER_PID,
    TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows_sys::Win32::Networking::WinSock::AF_INET;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

fn fetch(read: impl Fn(*mut core::ffi::c_void, *mut u32) -> u32) -> Option<Vec<u32>> {
    let mut size = 0u32;
    read(std::ptr::null_mut(), &mut size);
    for _ in 0..3 {
        size += 4096;
        let mut buf = vec![0u32; (size as usize).div_ceil(4)];
        match read(buf.as_mut_ptr().cast(), &mut size) {
            0 => return Some(buf),
            ERROR_INSUFFICIENT_BUFFER => continue,
            _ => return None,
        }
    }
    None
}

/// The rows of a table that starts with a u32 count, all fields u32.
fn rows<T: Copy>(buf: &[u32]) -> Vec<T> {
    let count = buf.first().copied().unwrap_or(0) as usize;
    let room = (buf.len().saturating_sub(1) * 4) / std::mem::size_of::<T>();
    let first = buf[1..].as_ptr().cast::<T>();
    (0..count.min(room))
        .map(|i| unsafe { first.add(i).read_unaligned() })
        .collect()
}

/// Which process owns each socket on the capture adapter, keyed by
/// (is TCP, local port).
pub fn tun_socket_owners(tun: Ipv4Addr) -> HashMap<(bool, u16), u32> {
    let addr = u32::from_ne_bytes(tun.octets());
    let port = |raw: u32| u16::from_be(raw as u16);
    let mut owners = HashMap::new();
    let tcp = fetch(|p, s| unsafe {
        GetExtendedTcpTable(p, s, 0, AF_INET as u32, TCP_TABLE_OWNER_PID_ALL, 0)
    });
    for r in tcp.iter().flat_map(|b| rows::<MIB_TCPROW_OWNER_PID>(b)) {
        if r.dwLocalAddr == addr {
            owners.insert((true, port(r.dwLocalPort)), r.dwOwningPid);
        }
    }
    let udp = fetch(|p, s| unsafe {
        GetExtendedUdpTable(p, s, 0, AF_INET as u32, UDP_TABLE_OWNER_PID, 0)
    });
    for r in udp.iter().flat_map(|b| rows::<MIB_UDPROW_OWNER_PID>(b)) {
        // UDP sockets are often bound to 0.0.0.0, so the port has to do.
        if r.dwLocalAddr == addr || r.dwLocalAddr == 0 {
            owners.entry((false, port(r.dwLocalPort))).or_insert(r.dwOwningPid);
        }
    }
    owners
}

/// The executable's file name, such as `chrome.exe`.
pub fn process_name(pid: u32) -> Option<String> {
    if pid == 0 || pid == 4 {
        return Some("System".into());
    }
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&path[..len as usize]);
        path.rsplit('\\').next().map(str::to_owned)
    }
}
