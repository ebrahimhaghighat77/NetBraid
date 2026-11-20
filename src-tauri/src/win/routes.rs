use std::io;
use std::net::Ipv4Addr;

use windows_sys::Win32::NetworkManagement::IpHelper::{
    CreateIpForwardEntry2, DeleteIpForwardEntry2, FreeMibTable, GetIpForwardTable2,
    InitializeIpForwardEntry, MIB_IPFORWARD_ROW2, MIB_IPFORWARD_TABLE2,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, MIB_IPPROTO_NETMGMT};

const ERROR_OBJECT_ALREADY_EXISTS: u32 = 5010;

fn row(if_index: u32, prefix: Ipv4Addr, len: u8) -> MIB_IPFORWARD_ROW2 {
    unsafe {
        let mut row: MIB_IPFORWARD_ROW2 = std::mem::zeroed();
        InitializeIpForwardEntry(&mut row);
        row.InterfaceIndex = if_index;
        row.DestinationPrefix.PrefixLength = len;
        row.DestinationPrefix.Prefix.Ipv4.sin_family = AF_INET;
        row.DestinationPrefix.Prefix.Ipv4.sin_addr.S_un.S_addr = u32::from_ne_bytes(prefix.octets());
        row.NextHop.Ipv4.sin_family = AF_INET;
        row.Metric = 0;
        row.Protocol = MIB_IPPROTO_NETMGMT;
        row
    }
}

/// A route that sends a range of addresses into the adapter, removed when dropped.
pub struct CaptureRoute {
    row: MIB_IPFORWARD_ROW2,
}

// The row is plain data with no pointers that outlive it.
unsafe impl Send for CaptureRoute {}
unsafe impl Sync for CaptureRoute {}

impl CaptureRoute {
    pub fn add(if_index: u32, prefix: Ipv4Addr, len: u8) -> io::Result<CaptureRoute> {
        let row = row(if_index, prefix, len);
        match unsafe { CreateIpForwardEntry2(&row) } {
            0 | ERROR_OBJECT_ALREADY_EXISTS => Ok(CaptureRoute { row }),
            code => Err(io::Error::from_raw_os_error(code as i32)),
        }
    }
}

impl Drop for CaptureRoute {
    fn drop(&mut self) {
        unsafe { DeleteIpForwardEntry2(&self.row) };
    }
}

pub fn delete_route(if_index: u32, prefix: Ipv4Addr, len: u8) -> bool {
    unsafe { DeleteIpForwardEntry2(&row(if_index, prefix, len)) == 0 }
}

/// Indexes of the interfaces with an IPv4 default route. PPPoE and mobile
/// broadband connections have one without a gateway address.
pub fn default_route_ifaces() -> Vec<u32> {
    let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    if unsafe { GetIpForwardTable2(AF_INET, &mut table) } != 0 || table.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    unsafe {
        let first = std::ptr::addr_of!((*table).Table).cast::<MIB_IPFORWARD_ROW2>();
        for row in std::slice::from_raw_parts(first, (*table).NumEntries as usize) {
            if row.DestinationPrefix.PrefixLength == 0 && !out.contains(&row.InterfaceIndex) {
                out.push(row.InterfaceIndex);
            }
        }
        FreeMibTable(table as *const core::ffi::c_void);
    }
    out
}
