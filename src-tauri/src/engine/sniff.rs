/// Stop looking for a site name after this many bytes.
pub const LIMIT: usize = 4096;

#[derive(Debug, PartialEq)]
pub enum Sniffed {
    Found(String),
    Nothing,
    NeedMore,
}

/// The site a connection is for, from a TLS ClientHello's server name or
/// an HTTP request's Host header.
pub fn host(data: &[u8]) -> Sniffed {
    match data.first() {
        None => Sniffed::NeedMore,
        Some(0x16) => tls(data),
        Some(b) if b.is_ascii_uppercase() => http(data),
        _ => Sniffed::Nothing,
    }
}

/// The part of a host name that names the site, so `cdn.example.co.uk` and
/// `www.example.co.uk` count as one.
pub fn site(host: &str) -> &str {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let n = labels.len();
    if n <= 2 {
        return host;
    }
    let keep = if labels[n - 1].len() == 2 && labels[n - 2].len() <= 3 { 3 } else { 2 };
    if n <= keep {
        return host;
    }
    let skip: usize = labels[..n - keep].iter().map(|l| l.len() + 1).sum();
    &host[skip..]
}

enum Stop {
    Short,
    Bad,
}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Stop> {
        let end = self.at.checked_add(n).ok_or(Stop::Bad)?;
        let part = self.data.get(self.at..end).ok_or(Stop::Short)?;
        self.at = end;
        Ok(part)
    }

    fn byte(&mut self) -> Result<u8, Stop> {
        Ok(self.take(1)?[0])
    }

    fn short(&mut self) -> Result<usize, Stop> {
        let b = self.take(2)?;
        Ok(usize::from(u16::from_be_bytes([b[0], b[1]])))
    }
}

fn tls(data: &[u8]) -> Sniffed {
    match server_name(data) {
        Ok(Some(name)) => Sniffed::Found(name),
        Ok(None) | Err(Stop::Bad) => Sniffed::Nothing,
        Err(Stop::Short) if data.len() >= LIMIT => Sniffed::Nothing,
        Err(Stop::Short) => Sniffed::NeedMore,
    }
}

fn server_name(data: &[u8]) -> Result<Option<String>, Stop> {
    let mut c = Cursor { data, at: 0 };
    if c.byte()? != 0x16 {
        return Err(Stop::Bad);
    }
    c.take(4)?;
    if c.byte()? != 1 {
        return Err(Stop::Bad);
    }
    c.take(3 + 2 + 32)?;
    let session = usize::from(c.byte()?);
    c.take(session)?;
    let suites = c.short()?;
    c.take(suites)?;
    let compression = usize::from(c.byte()?);
    c.take(compression)?;
    let extensions = c.short()?;
    let end = c.at + extensions;
    while c.at + 4 <= end {
        let kind = c.short()?;
        let len = c.short()?;
        let body = c.take(len)?;
        if kind == 0 {
            let mut names = Cursor { data: body, at: 0 };
            names.short().map_err(|_| Stop::Bad)?;
            if names.byte().map_err(|_| Stop::Bad)? != 0 {
                return Ok(None);
            }
            let len = names.short().map_err(|_| Stop::Bad)?;
            let name = names.take(len).map_err(|_| Stop::Bad)?;
            return Ok(std::str::from_utf8(name).ok().map(str::to_ascii_lowercase));
        }
    }
    Ok(None)
}

fn http(data: &[u8]) -> Sniffed {
    let text = String::from_utf8_lossy(data);
    let Some(head) = text.find("\r\n\r\n").map(|end| &text[..end]) else {
        return if data.len() >= LIMIT { Sniffed::Nothing } else { Sniffed::NeedMore };
    };
    let mut lines = head.split("\r\n");
    if !lines.next().is_some_and(|first| first.contains(" HTTP/1.")) {
        return Sniffed::Nothing;
    }
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        if name.trim().eq_ignore_ascii_case("host") {
            let host = without_port(value.trim()).to_ascii_lowercase();
            return if host.is_empty() { Sniffed::Nothing } else { Sniffed::Found(host) };
        }
    }
    Sniffed::Nothing
}

fn without_port(host: &str) -> &str {
    if let Some(rest) = host.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    match host.rsplit_once(':') {
        Some((name, port)) if port.parse::<u16>().is_ok() => name,
        _ => host,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_hello(name: &str) -> Vec<u8> {
        let mut sni = Vec::new();
        sni.extend_from_slice(&((name.len() + 3) as u16).to_be_bytes());
        sni.push(0);
        sni.extend_from_slice(&(name.len() as u16).to_be_bytes());
        sni.extend_from_slice(name.as_bytes());
        let mut ext = vec![0x00, 0x0b, 0x00, 0x02, 0x01, 0x00];
        ext.extend_from_slice(&[0x00, 0x00]);
        ext.extend_from_slice(&(sni.len() as u16).to_be_bytes());
        ext.extend_from_slice(&sni);
        let mut body = vec![0x03, 0x03];
        body.extend_from_slice(&[7; 32]);
        body.push(0);
        body.extend_from_slice(&[0x00, 0x02, 0x13, 0x01, 0x01, 0x00]);
        body.extend_from_slice(&(ext.len() as u16).to_be_bytes());
        body.extend_from_slice(&ext);
        let mut hs = vec![0x01];
        hs.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
        hs.extend_from_slice(&body);
        let mut record = vec![0x16, 0x03, 0x01];
        record.extend_from_slice(&(hs.len() as u16).to_be_bytes());
        record.extend_from_slice(&hs);
        record
    }

    #[test]
    fn reads_the_tls_server_name_once_it_has_arrived() {
        let hello = client_hello("Video.Example.com");
        assert_eq!(host(&hello), Sniffed::Found("video.example.com".into()));
        assert_eq!(host(&hello[..40]), Sniffed::NeedMore);
        assert_eq!(host(&[0x17, 3, 3]), Sniffed::Nothing);
    }

    #[test]
    fn reads_the_http_host_header() {
        let request = b"GET / HTTP/1.1\r\nUser-Agent: x\r\nHOST: example.com:8080\r\n\r\n";
        assert_eq!(host(request), Sniffed::Found("example.com".into()));
        assert_eq!(host(b"GET / HTTP/1.1\r\nHost: exa"), Sniffed::NeedMore);
        assert_eq!(host(b"EHLO mail\r\n\r\n"), Sniffed::Nothing);
        assert_eq!(without_port("[2001:db8::1]:443"), "2001:db8::1");
    }

    #[test]
    fn names_the_site_not_the_server() {
        assert_eq!(site("www.youtube.com"), "youtube.com");
        assert_eq!(site("a.b.example.co.uk"), "example.co.uk");
        assert_eq!(site("cdn.digikala.ac.ir"), "digikala.ac.ir");
        assert_eq!(site("x.google.ir"), "google.ir");
        assert_eq!(site("example.com"), "example.com");
        assert_eq!(site("1.2.3.4"), "1.2.3.4");
    }
}
