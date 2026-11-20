//! Origens de sites (protocolo + domínio + porta) e a regra de quando um login pode ser
//! preenchido numa página.
//! Website origins (scheme + domain + port) and the rule for when a login may be filled on a page.
//!
//! Regra / Rule ([`Origin::fills`]):
//! - protocolo igual: senha salva em `https` NUNCA vai para página `http` (nem o contrário);
//!   same scheme: a password saved on `https` NEVER goes to an `http` page (nor the reverse);
//! - porta igual; / same port;
//! - domínio igual. Subdomínios (`login.banco.com` para `banco.com`) só se o usuário permitir no item;
//!   same domain. Subdomains (`login.banco.com` for `banco.com`) only if the user allows it on the item;
//! - domínios com acentos viram punycode (`xn--…`), então `gооgle.com` com "о" cirílico não é
//!   `google.com`. / domains with accents become punycode (`xn--…`), so `gооgle.com` with a Cyrillic
//!   "о" is not `google.com`.

use url::{Host, Url};

/// Origem normalizada. / Normalized origin.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    scheme: String,
    /// Domínio em ASCII minúsculo (punycode) ou IP. / Lowercase ASCII domain (punycode) or IP.
    host: String,
    port: u16,
    is_domain: bool,
}

impl Origin {
    /// Lê uma origem. Sem protocolo, assume `https://` (`banco.com` → `https://banco.com`).
    /// Aceita só http e https. / Reads an origin. Without a scheme, assumes `https://`. Accepts only http and https.
    pub fn parse(input: &str) -> Option<Self> {
        let text = input.trim();
        if text.is_empty() || text.len() > 2048 {
            return None;
        }
        let url = if text.contains("://") { Url::parse(text).ok()? } else { Url::parse(&format!("https://{text}")).ok()? };
        let scheme = url.scheme();
        if scheme != "https" && scheme != "http" {
            return None;
        }
        let (host, is_domain) = match url.host()? {
            Host::Domain(d) => (d.trim_end_matches('.').to_owned(), true),
            Host::Ipv4(ip) => (ip.to_string(), false),
            Host::Ipv6(ip) => (format!("[{ip}]"), false),
        };
        if host.is_empty() {
            return None;
        }
        Some(Self { scheme: scheme.to_owned(), host, port: url.port_or_known_default()?, is_domain })
    }

    /// Protocolo (`https` ou `http`). / Scheme.
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Domínio em punycode (o que deve ser mostrado ao usuário). / Domain in punycode (what should be shown).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// A página é segura (https)? / Is the page secure (https)?
    pub fn is_secure(&self) -> bool {
        self.scheme == "https"
    }

    /// Texto canônico: `https://banco.com` (porta só se não for a padrão).
    /// Canonical text: `https://banco.com` (port only if not the default).
    pub fn to_canonical(&self) -> String {
        let default = if self.scheme == "https" { 443 } else { 80 };
        if self.port == default { format!("{}://{}", self.scheme, self.host) } else { format!("{}://{}:{}", self.scheme, self.host, self.port) }
    }

    /// Um login salvo nesta origem pode ser preenchido na página `page`?
    /// May a login saved at this origin be filled on page `page`?
    pub fn fills(&self, page: &Origin, allow_subdomains: bool) -> bool {
        if self.scheme != page.scheme || self.port != page.port {
            return false;
        }
        if self.host == page.host {
            return true;
        }
        // Subdomínio: só para domínios (não IPs), com ponto de fronteira, e com pelo menos dois rótulos
        // no domínio salvo (nunca "com" ou "com.br" inteiros).
        // Subdomain: only for domains (not IPs), with a dot boundary, and at least two labels in the saved
        // domain (never a whole "com" or "com.br").
        allow_subdomains
            && self.is_domain
            && page.is_domain
            && self.host.contains('.')
            && page.host.len() > self.host.len()
            && page.host.ends_with(&self.host)
            && page.host.as_bytes()[page.host.len() - self.host.len() - 1] == b'.'
    }
}

#[cfg(test)]
mod tests {
    use super::Origin;

    fn o(s: &str) -> Origin {
        Origin::parse(s).unwrap_or_else(|| panic!("não leu / not parsed: {s}"))
    }

    #[test]
    fn parses_and_normalizes() {
        assert_eq!(o("banco.com").to_canonical(), "https://banco.com");
        assert_eq!(o("HTTPS://Banco.COM/login?x=1#y").to_canonical(), "https://banco.com");
        assert_eq!(o("https://banco.com:443").to_canonical(), "https://banco.com");
        assert_eq!(o("http://localhost:8080/").to_canonical(), "http://localhost:8080");
        assert_eq!(o("https://banco.com.").host(), "banco.com");
        // Acentos e homógrafos viram punycode. / Accents and homographs become punycode.
        assert!(o("https://açaí.com.br").host().starts_with("xn--"));
        assert!(o("https://açaí.com.br").host().ends_with(".com.br"));
        assert!(o("https://gооgle.com").host().starts_with("xn--"));
        assert_ne!(o("https://gооgle.com"), o("https://google.com"));
        for bad in ["", "ftp://banco.com", "javascript:alert(1)", "file:///C:/x", "chrome-extension://abc/", "https://"] {
            assert!(Origin::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn exact_origin_rule() {
        let saved = o("https://banco.com");
        assert!(saved.fills(&o("https://banco.com/entrar"), false));
        // Protocolo, porta e domínio parecido. / Scheme, port and look-alike domain.
        assert!(!saved.fills(&o("http://banco.com"), false));
        assert!(!o("http://banco.com").fills(&o("https://banco.com"), false));
        assert!(!saved.fills(&o("https://banco.com:8443"), false));
        assert!(!saved.fills(&o("https://banco.com.golpe.net"), true));
        assert!(!saved.fills(&o("https://bancocom.net"), true));
        assert!(!saved.fills(&o("https://xbanco.com"), true));
        assert!(!o("https://google.com").fills(&o("https://gооgle.com"), true));
        // Subdomínio só com permissão. / Subdomain only with permission.
        assert!(!saved.fills(&o("https://login.banco.com"), false));
        assert!(saved.fills(&o("https://login.banco.com"), true));
        assert!(saved.fills(&o("https://a.b.banco.com"), true));
        // O contrário nunca: senha do subdomínio não vai para o domínio pai.
        // Never the reverse: a subdomain's password does not go to the parent domain.
        assert!(!o("https://login.banco.com").fills(&saved, true));
    }

    #[test]
    fn no_subdomain_tricks_with_ips_or_suffixes() {
        // "x.1.2.3.4" nem é um endereço válido (termina em número). / Not even a valid address (ends in a number).
        assert!(Origin::parse("http://x.1.2.3.4").is_none());
        assert!(!o("http://1.2.3.4").fills(&o("http://1.2.3.40"), true));
        assert!(!o("https://localhost").fills(&o("https://evil.localhost"), true));
        assert!(o("http://127.0.0.1:5000").fills(&o("http://127.0.0.1:5000/login"), false));
    }
}
