use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use super::TUN_ADDR;
use crate::win;

type Reading = (Instant, HashMap<(bool, u16), u32>);

/// Socket owners on the adapter. Reading the table takes a millisecond or
/// more, so flows opened before the last reading share it.
#[derive(Default)]
pub struct Owners {
    table: Mutex<Option<Reading>>,
}

impl Owners {
    pub fn app(&self, udp: bool, port: u16, opened: Instant) -> Option<String> {
        let mut table = self.table.lock().unwrap();
        if !matches!(&*table, Some((at, _)) if *at >= opened) {
            *table = Some((Instant::now(), win::tun_socket_owners(TUN_ADDR)));
        }
        let pid = *table.as_ref()?.1.get(&(!udp, port))?;
        drop(table);
        win::process_name(pid)
    }
}
