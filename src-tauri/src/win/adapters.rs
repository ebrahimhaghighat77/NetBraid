use std::ffi::CStr;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetIfEntry2, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST,
    GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH, MIB_IF_ROW2,
};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6, SOCKET_ADDRESS,
};

const ERROR_BUFFER_OVERFLOW: u32 = 111;
const IF_OPER_STATUS_UP: i32 = 1;
const FLAG_HARDWARE_INTERFACE: u8 = 0b0000_0001;

#[derive(Clone, Debug)]
pub struct Adapter {
    pub index: u32,
    pub guid: String,
    pub name: String,
    pub description: String,
    pub if_type: u32,
    pub up: bool,
    pub hardware: bool,
    pub ipv4: Vec<Ipv4Addr>,
    pub gateways_v4: Vec<Ipv4Addr>,
    pub dns: Vec<IpAddr>,
}

pub fn list_adapters() -> Vec<Adapter> {
    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 16 * 1024;
    for _ in 0..4 {
        let mut buf = vec![0u64; (size as usize).div_ceil(8)];
        let first = buf.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let rc = unsafe { GetAdaptersAddresses(0, flags, std::ptr::null(), first, &mut size) };
        match rc {
            0 => return unsafe { walk(first) },
            ERROR_BUFFER_OVERFLOW => continue,
            _ => return Vec::new(),
        }
    }
    Vec::new()
}

unsafe fn walk(mut node: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<Adapter> {
    let mut out = Vec::new();
    while let Some(a) = node.as_ref() {
        let index = a.Anonymous1.Anonymous.IfIndex;
        let mut adapter = Adapter {
            index,
            guid: narrow(a.AdapterName),
            name: wide(a.FriendlyName),
            description: wide(a.Description),
            if_type: a.IfType,
            up: a.OperStatus == IF_OPER_STATUS_UP,
            hardware: is_hardware(index),
            ipv4: Vec::new(),
            gateways_v4: Vec::new(),
            dns: Vec::new(),
        };
        let mut unicast = a.FirstUnicastAddress;
        while let Some(u) = unicast.as_ref() {
            if let Some(IpAddr::V4(ip)) = address(&u.Address) {
                adapter.ipv4.push(ip);
            }
            unicast = u.Next;
        }
        let mut gateway = a.FirstGatewayAddress;
        while let Some(g) = gateway.as_ref() {
            if let Some(IpAddr::V4(ip)) = address(&g.Address) {
                if !ip.is_unspecified() {
                    adapter.gateways_v4.push(ip);
                }
            }
            gateway = g.Next;
        }
        let mut dns = a.FirstDnsServerAddress;
        while let Some(d) = dns.as_ref() {
            if let Some(ip) = address(&d.Address) {
                adapter.dns.push(ip);
            }
            dns = d.Next;
        }
        out.push(adapter);
        node = a.Next;
    }
    out
}

unsafe fn address(addr: &SOCKET_ADDRESS) -> Option<IpAddr> {
    if addr.lpSockaddr.is_null() {
        return None;
    }
    let family = std::ptr::read_unaligned(addr.lpSockaddr.cast::<SOCKADDR>()).sa_family;
    match family {
        AF_INET => {
            let sin = std::ptr::read_unaligned(addr.lpSockaddr.cast::<SOCKADDR_IN>());
            Some(IpAddr::V4(Ipv4Addr::from(sin.sin_addr.S_un.S_addr.to_ne_bytes())))
        }
        AF_INET6 => {
            let sin6 = std::ptr::read_unaligned(addr.lpSockaddr.cast::<SOCKADDR_IN6>());
            Some(IpAddr::V6(Ipv6Addr::from(sin6.sin6_addr.u.Byte)))
        }
        _ => None,
    }
}

unsafe fn wide(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *text.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
}

unsafe fn narrow(text: *const u8) -> String {
    if text.is_null() {
        return String::new();
    }
    CStr::from_ptr(text.cast()).to_string_lossy().into_owned()
}

fn is_hardware(index: u32) -> bool {
    let mut row: MIB_IF_ROW2 = unsafe { std::mem::zeroed() };
    row.InterfaceIndex = index;
    unsafe {
        GetIfEntry2(&mut row) == 0
            && row.InterfaceAndOperStatusFlags._bitfield & FLAG_HARDWARE_INTERFACE != 0
    }
}
