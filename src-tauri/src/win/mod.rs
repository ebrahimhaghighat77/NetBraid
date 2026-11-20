mod adapters;
mod icmp;
mod owners;
mod policy;
mod routes;
mod sockets;

pub use adapters::{list_adapters, Adapter};
pub use icmp::echo_v4;
pub use owners::{process_name, tun_socket_owners};
pub use policy::Ipv4First;
pub use routes::{default_route_ifaces, delete_route, CaptureRoute};
pub use sockets::{bind_to_interface, is_msg_size};

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

/// Today's local date as (year, month, day).
pub fn local_date() -> (u16, u16, u16) {
    let mut now: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut now) };
    (now.wYear, now.wMonth, now.wDay)
}
