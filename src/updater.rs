//! Selbst-Update über GitHub Releases.
//!
//! Ablauf: neuestes Release abfragen → Version vergleichen → passende Datei
//! für das Betriebssystem herunterladen → laufendes Programm ersetzen →
//! neu starten. Für ein privates Repo ist ein GitHub-Token nötig
//! (Fine-grained Token mit Leserecht „Contents“ für dieses Repo).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const REPO: &str = "maniacmaik99/ygo-combomaker";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

/// Name der Release-Datei für dieses Betriebssystem.
pub fn asset_name() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("ygo-combomaker-windows-x86_64.exe")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("ygo-combomaker-linux-x86_64")
    } else {
        None
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    /// API-Adresse (funktioniert mit Token, auch bei privaten Repos).
    url: String,
    browser_download_url: String,
}

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    api_url: String,
    download_url: String,
}

/// Zerlegt "v1.2.3" oder "1.2.3" in Zahlen. Fehlende Teile zählen als 0.
fn parse_version(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn is_newer(remote: &str, current: &str) -> bool {
    let mut r = parse_version(remote);
    let mut c = parse_version(current);
    r.resize(3, 0);
    c.resize(3, 0);
    r > c
}

fn get(url: &str, token: &str, accept: &str) -> Result<ureq::http::Response<ureq::Body>> {
    let mut req = ureq::get(url)
        .header("User-Agent", crate::cards::USER_AGENT)
        .header("Accept", accept)
        .header("X-GitHub-Api-Version", "2022-11-28");
    if !token.trim().is_empty() {
        req = req.header("Authorization", &format!("Bearer {}", token.trim()));
    }
    match req.call() {
        Ok(r) => Ok(r),
        Err(ureq::Error::StatusCode(404)) => bail!(
            "Kein Release gefunden oder kein Zugriff. Bei einem privaten Repo muss unter \
             „Einstellungen“ ein gültiges GitHub-Token eingetragen sein."
        ),
        Err(ureq::Error::StatusCode(401)) => {
            bail!("Das GitHub-Token ist ungültig oder abgelaufen.")
        }
        Err(e) => Err(e).with_context(|| format!("Anfrage an {url} fehlgeschlagen")),
    }
}

/// Gibt `Some`, wenn es eine neuere Version mit passender Datei gibt.
pub fn check(token: &str) -> Result<Option<UpdateInfo>> {
    let Some(asset_name) = asset_name() else {
        bail!("Für dieses Betriebssystem gibt es keine automatischen Updates.");
    };
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = get(&url, token, "application/vnd.github+json")?;
    let release: Release = serde_json::from_slice(&resp.body_mut().read_to_vec()?)
        .context("Antwort von GitHub nicht lesbar")?;
    if !is_newer(&release.tag_name, CURRENT_VERSION) {
        return Ok(None);
    }
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .with_context(|| {
            format!(
                "Release {} enthält keine Datei „{asset_name}“.",
                release.tag_name
            )
        })?;
    Ok(Some(UpdateInfo {
        version: release.tag_name.trim_start_matches('v').to_string(),
        notes: release.body.unwrap_or_default(),
        api_url: asset.url.clone(),
        download_url: asset.browser_download_url.clone(),
    }))
}

/// Lädt das Update herunter und ersetzt das laufende Programm.
/// Gibt den Pfad zurück, unter dem das neue Programm gestartet werden kann.
pub fn install(info: &UpdateInfo, token: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("Pfad des Programms unbekannt")?;
    let (url, accept) = if token.trim().is_empty() {
        (info.download_url.as_str(), "application/octet-stream")
    } else {
        // Mit Token über die API laden; GitHub leitet auf einen Download-Server um.
        // ureq schickt das Token bei der Umleitung nicht mit (Standard).
        (info.api_url.as_str(), "application/octet-stream")
    };
    let mut resp = get(url, token, accept)?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()?;
    if bytes.len() < 100_000 {
        bail!(
            "Die heruntergeladene Datei ist zu klein ({} Byte). Update abgebrochen.",
            bytes.len()
        );
    }

    let tmp = exe.with_file_name(format!(
        ".ygo-combomaker-update-{}{}",
        info.version,
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::write(&tmp, &bytes)
        .with_context(|| format!("Schreiben von {} fehlgeschlagen", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    let result = self_replace::self_replace(&tmp).context("Programm konnte nicht ersetzt werden");
    let _ = std::fs::remove_file(&tmp);
    result?;
    Ok(exe)
}

/// Startet das (neue) Programm und beendet das laufende.
pub fn restart(exe: &std::path::Path) -> ! {
    let _ = std::process::Command::new(exe).spawn();
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("v1.0", "0.9.9"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
    }

    #[test]
    fn asset_exists_for_this_platform() {
        assert!(asset_name().is_some());
    }
}
