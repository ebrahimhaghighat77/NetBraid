use std::net::IpAddr;

use serde::{Deserialize, Serialize};

pub const BLOCK: &str = "block";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Rule {
    pub app: String,
    // A site also covers its subdomains; an IP address matches exactly. Empty matches all.
    pub host: String,
    // A network id, or BLOCK.
    pub link: String,
    pub fallback: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Any,
    Block,
    Only { link: String, fallback: bool },
}

fn host_matches(host: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_start_matches("*.");
    let Some(start) = host.len().checked_sub(pattern.len()) else {
        return false;
    };
    host.is_char_boundary(start)
        && host[start..].eq_ignore_ascii_case(pattern)
        && (start == 0 || host.as_bytes()[start - 1] == b'.')
}

impl Rule {
    fn matches(&self, app: Option<&str>, host: Option<&str>, ip: IpAddr) -> bool {
        if self.app.is_empty() && self.host.is_empty() {
            return false;
        }
        let app_ok = self.app.is_empty() || app.is_some_and(|a| a.eq_ignore_ascii_case(&self.app));
        let host_ok = self.host.is_empty()
            || host.is_some_and(|h| host_matches(h, &self.host))
            || self.host.parse::<IpAddr>().is_ok_and(|want| want == ip);
        app_ok && host_ok
    }
}

pub fn verdict(rules: &[Rule], app: Option<&str>, host: Option<&str>, ip: IpAddr) -> Verdict {
    match rules.iter().find(|r| r.matches(app, host, ip)) {
        None => Verdict::Any,
        Some(rule) if rule.link == BLOCK => Verdict::Block,
        Some(rule) => Verdict::Only {
            link: rule.link.clone(),
            fallback: rule.fallback,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(93, 184, 216, 34));

    fn rule(app: &str, host: &str, link: &str) -> Rule {
        Rule {
            app: app.into(),
            host: host.into(),
            link: link.into(),
            fallback: false,
        }
    }

    fn only(link: &str) -> Verdict {
        Verdict::Only {
            link: link.into(),
            fallback: false,
        }
    }

    #[test]
    fn a_site_rule_covers_its_subdomains_only() {
        let rules = [rule("", "example.com", "wifi")];
        assert_eq!(verdict(&rules, None, Some("example.com"), IP), only("wifi"));
        assert_eq!(verdict(&rules, None, Some("CDN.Example.com"), IP), only("wifi"));
        assert_eq!(verdict(&rules, None, Some("notexample.com"), IP), Verdict::Any);
        assert_eq!(verdict(&rules, None, None, IP), Verdict::Any);
    }

    #[test]
    fn an_app_rule_ignores_case_and_needs_the_app() {
        let rules = [rule("Game.exe", "", "ethernet")];
        assert_eq!(verdict(&rules, Some("game.exe"), None, IP), only("ethernet"));
        assert_eq!(verdict(&rules, Some("chrome.exe"), None, IP), Verdict::Any);
        assert_eq!(verdict(&rules, None, None, IP), Verdict::Any);
    }

    #[test]
    fn the_first_match_wins_and_can_block() {
        let rules = [
            rule("chrome.exe", "ads.example", BLOCK),
            rule("chrome.exe", "", "wifi"),
            rule("", "93.184.216.34", "phone"),
        ];
        let chrome = Some("chrome.exe");
        assert_eq!(verdict(&rules, chrome, Some("x.ads.example"), IP), Verdict::Block);
        assert_eq!(verdict(&rules, chrome, Some("example.com"), IP), only("wifi"));
        assert_eq!(verdict(&rules, Some("curl.exe"), None, IP), only("phone"));
    }

    #[test]
    fn an_empty_rule_matches_nothing() {
        assert_eq!(verdict(&[rule("", "", "wifi")], Some("a.exe"), Some("b.c"), IP), Verdict::Any);
    }
}
