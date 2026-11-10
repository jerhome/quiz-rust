use quiz_net::{LocalAddr, PlayerInfo, WebAssets};

use crate::multi;

type Asset = (&'static str, &'static [u8], &'static str);

const HTML: &str = "text/html; charset=utf-8";
const PNG: &str = "image/png";
const FONT: &str = "font/woff2";

pub const FILES: &[Asset] = &[
    ("index.html", include_bytes!("../web/index.html"), HTML),
    (
        "app.js",
        include_bytes!("../web/app.js"),
        "text/javascript; charset=utf-8",
    ),
    (
        "style.css",
        include_bytes!("../web/style.css"),
        "text/css; charset=utf-8",
    ),
    (
        "manifest.webmanifest",
        include_bytes!("../web/manifest.webmanifest"),
        "application/manifest+json",
    ),
    (
        "icon.svg",
        include_bytes!("../web/icon.svg"),
        "image/svg+xml",
    ),
    ("icon-192.png", include_bytes!("../web/icon-192.png"), PNG),
    ("icon-512.png", include_bytes!("../web/icon-512.png"), PNG),
    (
        "apple-touch-icon.png",
        include_bytes!("../web/apple-touch-icon.png"),
        PNG,
    ),
    (
        "fonts/inter-400.woff2",
        include_bytes!("../web/fonts/inter-400.woff2"),
        FONT,
    ),
    (
        "fonts/inter-600.woff2",
        include_bytes!("../web/fonts/inter-600.woff2"),
        FONT,
    ),
    (
        "fonts/bricolage-grotesque.woff2",
        include_bytes!("../web/fonts/bricolage-grotesque.woff2"),
        FONT,
    ),
];

pub fn assets() -> WebAssets {
    FILES
        .iter()
        .fold(WebAssets::new(), |a, (path, body, mime)| {
            a.with(path, body, mime)
        })
}

pub fn web_url(addrs: &[LocalAddr], port: u16) -> Option<String> {
    let ip = multi::primary_addr(addrs)?.ip;
    Some(format!("http://{ip}:{port}/"))
}

/// Row-major dark modules and the side length, for painting without an image crate.
pub fn qr_modules(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let side = code.width();
    let dark = code
        .to_colors()
        .into_iter()
        .map(|c| c == qrcode::Color::Dark)
        .collect();
    Some((side, dark))
}

pub fn connected_count(players: &[PlayerInfo]) -> usize {
    players.iter().filter(|p| p.connected).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiz_net::Device;
    use std::net::Ipv4Addr;

    fn addr(ip: [u8; 4], virt: bool) -> LocalAddr {
        LocalAddr {
            ip: Ipv4Addr::from(ip),
            interface_name: "eth0".into(),
            is_likely_virtual: virt,
        }
    }

    fn player(id: u32, connected: bool) -> PlayerInfo {
        PlayerInfo {
            id,
            name: format!("p{id}"),
            is_host: id == 0,
            device: Device::Web,
            connected,
        }
    }

    #[test]
    fn url_uses_primary_address_and_web_port() {
        let addrs = [addr([172, 17, 0, 1], true), addr([192, 168, 1, 20], false)];
        assert_eq!(
            web_url(&addrs, 7880).as_deref(),
            Some("http://192.168.1.20:7880/")
        );
        assert_eq!(web_url(&[], 7880), None);
    }

    #[test]
    fn assets_are_registered_with_mimes() {
        let find = |p: &str| {
            FILES
                .iter()
                .find(|f| f.0 == p)
                .unwrap_or_else(|| panic!("{p}"))
        };
        for (path, mime) in [
            ("index.html", "text/html; charset=utf-8"),
            ("app.js", "text/javascript; charset=utf-8"),
            ("style.css", "text/css; charset=utf-8"),
            ("manifest.webmanifest", "application/manifest+json"),
            ("fonts/inter-400.woff2", "font/woff2"),
            ("apple-touch-icon.png", "image/png"),
        ] {
            let (_, body, m) = find(path);
            assert!(!body.is_empty(), "{path}");
            assert_eq!(*m, mime, "{path}");
        }
        let total: usize = FILES.iter().map(|f| f.1.len()).sum();
        assert!(total < 1_000_000, "{total}");
        let _ = assets();
    }

    #[test]
    fn counts_only_connected_players() {
        let players = [player(0, true), player(1, false), player(2, true)];
        assert_eq!(connected_count(&players), 2);
    }

    #[test]
    fn qr_is_square() {
        let (side, dark) = qr_modules("http://192.168.1.20:7880/").unwrap();
        assert_eq!(dark.len(), side * side);
    }
}
