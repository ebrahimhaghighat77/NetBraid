use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;

/// Passed by the sign-in task, so the app starts quietly in the tray.
pub const ARG: &str = "--autostart";
const TASK: &str = "NetBraid";
const NO_WINDOW: u32 = 0x0800_0000;

fn schtasks(args: &[&str]) -> io::Result<()> {
    let out = Command::new("schtasks").args(args).creation_flags(NO_WINDOW).output()?;
    if out.status.success() {
        Ok(())
    } else {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(io::Error::other(why))
    }
}

pub fn exists() -> bool {
    schtasks(&["/Query", "/TN", TASK]).is_ok()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A task rather than a Run key: it starts the app with administrator
/// rights at sign-in without asking, and doesn't care about battery power.
fn task_xml(exe: &str, user: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Starts NetBraid when you sign in.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT5S</Delay></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command><Arguments>{ARG}</Arguments></Exec></Actions>
</Task>
"#,
        exe = escape(exe),
        user = escape(user),
    )
}

pub fn enable() -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(domain), Ok(name)) => format!("{domain}\\{name}"),
        (_, Ok(name)) => name,
        _ => return Err(io::Error::other("the signed-in user is unknown")),
    };
    let xml = task_xml(&exe.to_string_lossy(), &user);
    // schtasks reads the file as UTF-16, so it is written that way, with a BOM.
    let mut bytes = vec![0xff, 0xfe];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    let path = std::env::temp_dir().join("netbraid-task.xml");
    std::fs::write(&path, bytes)?;
    let created = schtasks(&["/Create", "/TN", TASK, "/XML", &path.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&path);
    created
}

pub fn disable() -> io::Result<()> {
    if exists() {
        schtasks(&["/Delete", "/TN", TASK, "/F"])
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_task_escapes_paths_and_names() {
        let xml = task_xml(r"C:\Program Files\R&D <x>\netbraid.exe", r"PC\me");
        assert!(xml.contains(r"<Command>C:\Program Files\R&amp;D &lt;x&gt;\netbraid.exe</Command>"));
        assert!(xml.contains(r"<UserId>PC\me</UserId>"));
        assert!(xml.contains("<Arguments>--autostart</Arguments>"));
    }
}
