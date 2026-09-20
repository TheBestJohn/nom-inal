//! Fetching a page somebody else chose.
//!
//! `POST /recipes/import` takes a URL from the caller and makes the server
//! request it, which is the textbook server-side request forgery: the server
//! sits inside the deployment's network and a caller does not. So the fetch
//! is fenced on every side before a byte is read:
//!
//! - only `http` and `https`, no credentials in the URL;
//! - the host is resolved *here*, every address it resolves to has to be
//!   globally routable, and the request is pinned to exactly those addresses
//!   so a name that resolves differently a moment later (DNS rebinding)
//!   cannot reach a different one;
//! - redirects are not followed by the client; each hop comes back here and
//!   goes through the same checks, and there are at most four;
//! - the response has to be a web page or JSON, and is read in chunks up to
//!   a cap so an endless body cannot fill memory;
//! - one timeout for the whole thing.
//!
//! Proxy environment variables are honoured as they are for the food
//! providers; behind a proxy the address pinning is moot (the proxy resolves
//! the name), but the checks on what was asked for still apply.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::redirect;
use url::{Host, Url};

use crate::error::{ApiError, ApiResult};

/// Largest page read, in bytes. Recipe pages with all their scripts run to a
/// megabyte or so; the JSON-LD block is a few kilobytes of that.
const MAX_BYTES: usize = 3 * 1024 * 1024;
const MAX_HOPS: usize = 4;
const OVERALL_TIMEOUT: Duration = Duration::from_secs(20);
const HOP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct FetchedPage {
    /// Where the page came from after any redirects.
    pub url: String,
    pub content_type: String,
    pub body: String,
}

/// Fetch a public web page, with the checks in the module note.
pub async fn fetch_public_page(raw: &str) -> ApiResult<FetchedPage> {
    tokio::time::timeout(OVERALL_TIMEOUT, fetch_inner(raw))
        .await
        .unwrap_or_else(|_| {
            Err(ApiError::UpstreamUnavailable(
                "that page took too long to fetch".into(),
            ))
        })
}

async fn fetch_inner(raw: &str) -> ApiResult<FetchedPage> {
    let mut url = parse_url(raw)?;

    for _ in 0..=MAX_HOPS {
        let (host, addrs) = resolve_checked(&url).await?;

        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "nom-inal/",
                env!("CARGO_PKG_VERSION"),
                " (recipe import; +https://github.com/TheBestJohn/nom-inal)"
            ))
            .redirect(redirect::Policy::none())
            .resolve_to_addrs(&host, &addrs)
            .connect_timeout(Duration::from_secs(5))
            .timeout(HOP_TIMEOUT)
            .build()
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("building fetch client: {e}")))?;

        let response = client
            .get(url.as_str())
            .header(
                reqwest::header::ACCEPT,
                "text/html, application/xhtml+xml, application/ld+json, application/json;q=0.9",
            )
            .send()
            .await
            .map_err(|e| {
                ApiError::UpstreamUnavailable(format!(
                    "could not fetch that page: {}",
                    describe(&e)
                ))
            })?;

        let status = response.status();
        if status.is_redirection() {
            let Some(location) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            else {
                return Err(ApiError::bad_request(
                    "that page redirected somewhere it did not say",
                ));
            };
            let next = url.join(location).map_err(|_| {
                ApiError::bad_request("that page redirected to an address that is not a URL")
            })?;
            // The next hop is checked at the top of the loop, exactly as the
            // first was: a redirect is just another URL the caller chose.
            url = parse_url(next.as_str())?;
            continue;
        }

        if !status.is_success() {
            return Err(ApiError::UpstreamUnavailable(format!(
                "that page answered {}{}",
                status.as_u16(),
                status
                    .canonical_reason()
                    .map(|r| format!(" {r}"))
                    .unwrap_or_default()
            )));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let kind = content_type.split(';').next().unwrap_or("").trim();
        if !matches!(
            kind,
            "text/html" | "application/xhtml+xml" | "application/ld+json" | "application/json"
        ) {
            return Err(ApiError::bad_request(format!(
                "that URL is not a web page (it is {})",
                if kind.is_empty() {
                    "of no stated type"
                } else {
                    kind
                }
            )));
        }

        // Read in chunks against the cap: `Content-Length` is a claim, and a
        // server that lies about it or streams forever must not fill memory.
        let mut body: Vec<u8> = Vec::new();
        let mut response = response;
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            ApiError::UpstreamUnavailable(format!("the page cut off while being read: {e}"))
        })? {
            if body.len() + chunk.len() > MAX_BYTES {
                return Err(ApiError::bad_request(format!(
                    "that page is larger than {} MB",
                    MAX_BYTES / (1024 * 1024)
                )));
            }
            body.extend_from_slice(&chunk);
        }

        return Ok(FetchedPage {
            url: url.to_string(),
            content_type,
            body: String::from_utf8_lossy(&body).into_owned(),
        });
    }

    Err(ApiError::bad_request(format!(
        "that page redirected more than {MAX_HOPS} times"
    )))
}

/// Parse and apply the checks that need no network: scheme, credentials, a
/// host at all.
fn parse_url(raw: &str) -> ApiResult<Url> {
    let url = Url::parse(raw.trim())
        .map_err(|_| ApiError::bad_request("that does not look like a URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::bad_request(
            "only http and https pages can be imported",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ApiError::bad_request(
            "a URL with a username or password in it cannot be imported",
        ));
    }
    if url.host().is_none() {
        return Err(ApiError::bad_request("that URL has no host"));
    }
    Ok(url)
}

/// Resolve the URL's host and refuse it unless every address is one the
/// public internet could reach. Returns the host name as the client needs
/// it for pinning, and the addresses to pin.
async fn resolve_checked(url: &Url) -> ApiResult<(String, Vec<SocketAddr>)> {
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::bad_request("that URL has no port"))?;
    let refuse = || {
        ApiError::bad_request(
            "that address is private, local or reserved; only public web pages can be imported",
        )
    };

    let (host, addrs): (String, Vec<SocketAddr>) = match url.host() {
        Some(Host::Ipv4(ip)) => (ip.to_string(), vec![SocketAddr::new(ip.into(), port)]),
        Some(Host::Ipv6(ip)) => (ip.to_string(), vec![SocketAddr::new(ip.into(), port)]),
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            // Names that only ever mean "this machine" or "this network" are
            // refused by name, before a resolver gets a say.
            if name == "localhost"
                || name.ends_with(".localhost")
                || name.ends_with(".local")
                || name.ends_with(".internal")
                || name.ends_with(".home.arpa")
            {
                return Err(refuse());
            }
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((name.as_str(), port))
                .await
                .map_err(|_| ApiError::UpstreamUnavailable(format!("could not resolve {name}")))?
                .collect();
            if resolved.is_empty() {
                return Err(ApiError::UpstreamUnavailable(format!(
                    "could not resolve {name}"
                )));
            }
            (name, resolved)
        }
        None => return Err(ApiError::bad_request("that URL has no host")),
    };

    // Every address, not just the first: a name that resolves to one public
    // and one private address would otherwise be a coin toss.
    if addrs.iter().any(|a| !is_globally_routable(a.ip())) {
        return Err(refuse());
    }

    Ok((host, addrs))
}

/// Whether an address is one the public internet routes to. Everything
/// loopback, private, link-local (the cloud metadata service lives there),
/// carrier-grade NAT, multicast, documentation or reserved is not.
pub fn is_globally_routable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_global_v4(v4),
        IpAddr::V6(v6) => is_global_v6(v6),
    }
}

fn is_global_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        // 0.0.0.0/8 "this network"
        || a == 0
        // 100.64.0.0/10 shared address space (carrier-grade NAT)
        || (a == 100 && (64..=127).contains(&b))
        // 192.0.0.0/24 IETF protocol assignments
        || (a == 192 && b == 0 && c == 0)
        // 198.18.0.0/15 benchmarking
        || (a == 198 && (b == 18 || b == 19))
        // 224.0.0.0/4 multicast and 240.0.0.0/4 reserved
        || a >= 224)
}

fn is_global_v6(ip: Ipv6Addr) -> bool {
    // An IPv4 address carried inside IPv6 is judged as the IPv4 address:
    // ::ffff:127.0.0.1 is still loopback, and 64:ff9b::/96 (NAT64) embeds
    // one in the low 32 bits.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_global_v4(v4);
    }
    let segments = ip.segments();
    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let [_, _, _, _, _, _, hi, lo] = segments;
        return is_global_v4(Ipv4Addr::from(((hi as u32) << 16) | lo as u32));
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        // fc00::/7 unique local
        || (segments[0] & 0xfe00) == 0xfc00
        // fe80::/10 link-local
        || (segments[0] & 0xffc0) == 0xfe80
        // ff00::/8 multicast
        || (segments[0] & 0xff00) == 0xff00
        // 2001:db8::/32 documentation
        || (segments[0] == 0x2001 && segments[1] == 0xdb8)
        // ::/96 deprecated IPv4-compatible, and anything else in ::/64
        || segments[..4] == [0, 0, 0, 0])
}

/// A reqwest error's message carries the URL, which the caller already has;
/// the underlying cause is the useful part.
fn describe(e: &reqwest::Error) -> String {
    let mut source: &dyn std::error::Error = e;
    while let Some(next) = source.source() {
        source = next;
    }
    let text = source.to_string();
    if text.is_empty() {
        e.to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn public_addresses_pass() {
        for s in [
            "93.184.216.34",
            "8.8.8.8",
            "2606:4700::1111",
            "2001:4860:4860::8888",
        ] {
            assert!(is_globally_routable(ip(s)), "{s}");
        }
    }

    #[test]
    fn private_local_and_reserved_are_refused() {
        for s in [
            "127.0.0.1",
            "127.1.2.3",
            "0.0.0.0",
            "10.0.0.1",
            "172.16.5.5",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "192.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::7f00:1",
        ] {
            assert!(!is_globally_routable(ip(s)), "{s}");
        }
    }

    #[test]
    fn url_checks_need_no_network() {
        assert!(parse_url("ftp://example.com/x").is_err());
        assert!(parse_url("file:///etc/passwd").is_err());
        assert!(parse_url("javascript:alert(1)").is_err());
        assert!(parse_url("http://user:pw@example.com/").is_err());
        assert!(parse_url("not a url").is_err());
        assert!(parse_url("https://example.com/recipe").is_ok());
    }

    #[tokio::test]
    async fn loopback_and_local_names_are_refused_before_any_request() {
        for raw in [
            "http://127.0.0.1:8080/",
            "http://[::1]/",
            "http://localhost/",
            "http://api.localhost/",
            "http://printer.local/",
            "http://db.internal/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.1.2.3/",
        ] {
            let url = parse_url(raw).unwrap();
            let err = resolve_checked(&url).await.expect_err(raw);
            assert!(
                matches!(err, ApiError::BadRequest(ref m) if m.contains("private")),
                "{raw}: {err:?}"
            );
        }
    }
}
