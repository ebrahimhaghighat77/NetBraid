use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A local date as (year, month, day); day 0 stands for the whole month.
pub type Day = (u16, u16, u16);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tally {
    pub period: Day,
    pub bytes: u64,
}

/// Data carried today across every network, and per network for its cap.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Usage {
    pub day: Day,
    pub today: u64,
    pub links: HashMap<String, Tally>,
}

impl Usage {
    pub fn count_today(&mut self, day: Day, bytes: u64) {
        if self.day != day {
            self.day = day;
            self.today = 0;
        }
        self.today += bytes;
    }

    pub fn today(&self, day: Day) -> u64 {
        if self.day == day {
            self.today
        } else {
            0
        }
    }

    /// Adds to a network's count for `period`, starting over in a new one,
    /// and returns the new count.
    pub fn count_link(&mut self, id: &str, period: Day, bytes: u64) -> u64 {
        let tally = self.links.entry(id.to_owned()).or_default();
        if tally.period != period {
            *tally = Tally { period, bytes: 0 };
        }
        tally.bytes += bytes;
        tally.bytes
    }

    pub fn used(&self, id: &str, period: Day) -> u64 {
        self.links
            .get(id)
            .filter(|t| t.period == period)
            .map_or(0, |t| t.bytes)
    }

    pub fn reset(&mut self, id: &str) {
        self.links.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn today_starts_over_at_midnight() {
        let mut usage = Usage::default();
        usage.count_today((2026, 11, 20), 100);
        usage.count_today((2026, 11, 20), 50);
        assert_eq!(usage.today((2026, 11, 20)), 150);
        assert_eq!(usage.today((2026, 11, 21)), 0);
        usage.count_today((2026, 11, 21), 7);
        assert_eq!(usage.today((2026, 11, 21)), 7);
    }

    #[test]
    fn a_network_counts_for_its_period_until_reset() {
        let mut usage = Usage::default();
        assert_eq!(usage.count_link("phone", (2026, 11, 0), 100), 100);
        assert_eq!(usage.count_link("phone", (2026, 11, 0), 20), 120);
        assert_eq!(usage.used("phone", (2026, 11, 0)), 120);
        assert_eq!(usage.count_link("phone", (2026, 12, 0), 5), 5);
        usage.reset("phone");
        assert_eq!(usage.used("phone", (2026, 12, 0)), 0);
    }
}
