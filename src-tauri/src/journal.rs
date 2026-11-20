use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const FILE: &str = "netbraid.log";
const OLDER_FILE: &str = "netbraid.old.log";
const ROTATE_AT: u64 = 2 * 1024 * 1024;
const KEEP_LINES: usize = 400;

struct Journal {
    file: Mutex<Option<File>>,
    recent: Mutex<VecDeque<String>>,
}

static JOURNAL: OnceLock<Journal> = OnceLock::new();

/// Logs to a file in `dir`, keeping one older file once it fills up.
pub fn start(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let path = dir.join(FILE);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_AT) {
        let _ = std::fs::rename(&path, dir.join(OLDER_FILE));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok();
    let journal = JOURNAL.get_or_init(|| Journal {
        file: Mutex::new(file),
        recent: Mutex::new(VecDeque::new()),
    });
    if log::set_logger(journal).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    log::info!("NetBraid {} started", env!("CARGO_PKG_VERSION"));
}

/// The latest lines, oldest first.
pub fn recent() -> Vec<String> {
    JOURNAL
        .get()
        .map(|j| j.recent.lock().unwrap().iter().cloned().collect())
        .unwrap_or_default()
}

impl log::Log for Journal {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.target().starts_with("netbraid")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!("{} {:5} {}", utc_now(), record.level(), record.args());
        if let Some(file) = self.file.lock().unwrap().as_mut() {
            let _ = writeln!(file, "{line}");
        }
        let mut recent = self.recent.lock().unwrap();
        if recent.len() == KEEP_LINES {
            recent.pop_front();
        }
        recent.push_back(line);
    }

    fn flush(&self) {}
}

fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, mo, d) = calendar(secs / 86_400);
    let (h, mi, s) = (secs / 3600 % 24, secs / 60 % 60, secs % 60);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}Z")
}

// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn calendar(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + u64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_become_dates() {
        assert_eq!(calendar(0), (1970, 1, 1));
        assert_eq!(calendar(19_782), (2024, 2, 29));
        assert_eq!(calendar(20_088), (2024, 12, 31));
    }
}
