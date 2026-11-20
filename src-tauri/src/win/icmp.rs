use std::net::Ipv4Addr;
use std::time::Duration;

use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::NetworkManagement::IpHelper::{
    IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho2Ex, ICMP_ECHO_REPLY, IP_OPTION_INFORMATION,
    IP_SUCCESS,
};

/// Sends one echo request from `from` and returns the round trip in ms.
pub fn echo_v4(from: Ipv4Addr, to: Ipv4Addr, data: &[u8], ttl: u8, wait: Duration) -> Option<u32> {
    unsafe {
        let handle = IcmpCreateFile();
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return None;
        }
        let size = std::mem::size_of::<ICMP_ECHO_REPLY>() + data.len() + 64;
        let mut reply = vec![0u64; size.div_ceil(8)];
        let options = IP_OPTION_INFORMATION {
            Ttl: ttl,
            Tos: 0,
            Flags: 0,
            OptionsSize: 0,
            OptionsData: std::ptr::null_mut(),
        };
        let answers = IcmpSendEcho2Ex(
            handle,
            std::ptr::null_mut(),
            None,
            std::ptr::null(),
            u32::from_ne_bytes(from.octets()),
            u32::from_ne_bytes(to.octets()),
            data.as_ptr().cast(),
            data.len() as u16,
            &options,
            reply.as_mut_ptr().cast(),
            (reply.len() * 8) as u32,
            wait.as_millis() as u32,
        );
        IcmpCloseHandle(handle);
        if answers == 0 {
            return None;
        }
        let first = reply.as_ptr().cast::<ICMP_ECHO_REPLY>().read();
        (first.Status == IP_SUCCESS).then_some(first.RoundTripTime)
    }
}
