use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;

const NO_WINDOW: u32 = 0x0800_0000;
const MAPPED_V4: &str = "::ffff:0:0/96";
const MAPPED_V4_LABEL: &str = "4";
const WINDOWS_DEFAULT: u32 = 35;
/// Above the 40 of `::/0`, so Windows picks a site's IPv4 address over its
/// IPv6 one, and the connection reaches the capture adapter.
const RAISED: u32 = 45;

/// Raises IPv4 in Windows' address selection table while it lives. Only the
/// active store changes, so a reboot undoes it as well.
pub struct Ipv4First {
    before: u32,
}

impl Ipv4First {
    pub fn apply() -> io::Result<Ipv4First> {
        let before = current().filter(|p| *p != RAISED).unwrap_or(WINDOWS_DEFAULT);
        set(RAISED)?;
        Ok(Ipv4First { before })
    }
}

impl Drop for Ipv4First {
    fn drop(&mut self) {
        if let Err(err) = set(self.before) {
            log::warn!("could not restore the IPv4 preference: {err}");
        }
    }
}

fn netsh(args: &[&str]) -> io::Result<String> {
    let out = Command::new("netsh").args(args).creation_flags(NO_WINDOW).output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        Err(io::Error::other(text.trim().to_owned()))
    }
}

fn current() -> Option<u32> {
    let table = netsh(&["interface", "ipv6", "show", "prefixpolicies", "store=active"]).ok()?;
    precedence_of_mapped_v4(&table)
}

fn precedence_of_mapped_v4(table: &str) -> Option<u32> {
    table.lines().find_map(|line| {
        let cells: Vec<&str> = line.split_whitespace().collect();
        match cells.as_slice() {
            [precedence, _label, prefix] if *prefix == MAPPED_V4 => precedence.parse().ok(),
            _ => None,
        }
    })
}

fn set(precedence: u32) -> io::Result<()> {
    let precedence = precedence.to_string();
    netsh(&[
        "interface",
        "ipv6",
        "set",
        "prefixpolicy",
        MAPPED_V4,
        &precedence,
        MAPPED_V4_LABEL,
        "store=active",
    ])
    .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_ipv4_row_in_any_language() {
        let table = "Abfrage des aktiven Status...\n\n Rangfolge  Bezeichnung  Präfix\n ----------  -----  ----\n        50      0  ::1/128\n        40      1  ::/0\n        35      4  ::ffff:0:0/96\n";
        assert_eq!(precedence_of_mapped_v4(table), Some(35));
        assert_eq!(precedence_of_mapped_v4("no table"), None);
    }
}
