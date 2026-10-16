//! GitHub release discovery only. Never executes an installer, replaces a running
//! binary, or sends library/model data. Downloads open in the system browser.
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path, time::Duration};

pub const VERSION: &str = match option_env!("VELLIO_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};
pub const REPOSITORY: &str = "https://github.com/ohmycloud/vellio";
pub const RELEASES: &str = "https://github.com/ohmycloud/vellio/releases";
pub const ISSUES: &str = "https://github.com/ohmycloud/vellio/issues";
const API: &str = "https://api.github.com/repos/ohmycloud/vellio/releases/latest";
const DAY: i64 = 24 * 60 * 60;
const RETRY: i64 = 3 * 60 * 60;
const MAX_RESPONSE: u64 = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub notes: String,
    pub published: String,
    pub assets: Vec<Asset>,
}
impl Release {
    pub fn is_newer_than(&self, current: &str) -> bool {
        match (parse_version(&self.version), parse_version(current)) {
            (Ok(latest), Ok(current)) => latest.cmp_precedence(&current).is_gt(),
            _ => false,
        }
    }
    /// Exact release artifact names prevent e.g. an ARM machine from downloading
    /// an x86 installer. Universal macOS packages are also accepted.
    pub fn download(&self, os: &str, arch: &str) -> Option<&Asset> {
        let extensions: &[&str] = match os {
            "macos" => &["dmg", "zip"],
            "windows" => &["exe", "msi", "zip"],
            "linux" => &["AppImage", "tar.gz"],
            _ => return None,
        };
        if !matches!(arch, "aarch64" | "x86_64") {
            return None;
        }
        for extension in extensions {
            for cpu in [arch, "universal"] {
                if cpu == "universal" && os != "macos" {
                    continue;
                }
                let name = format!("Vellio-{}-{os}-{cpu}.{extension}", self.version);
                if let Some(asset) = self
                    .assets
                    .iter()
                    .find(|asset| asset.name == name && trusted_download(&asset.url, &asset.name))
                {
                    return Some(asset);
                }
            }
        }
        None
    }
}

pub fn parse_version(version: &str) -> Result<Version> {
    Version::parse(version.trim().strip_prefix('v').unwrap_or(version.trim()))
        .context("Release tag must be a semantic version, e.g. v1.2.3")
}
fn github_path(url: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("github.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| url.path().to_owned())
}
fn trusted_download(url: &str, name: &str) -> bool {
    github_path(url).is_some_and(|path| {
        let prefix = "/ohmycloud/vellio/releases/download/";
        path.strip_prefix(prefix).is_some_and(|rest| {
            rest.split_once('/')
                .is_some_and(|(tag, file)| parse_version(tag).is_ok() && file == name)
        })
    })
}
/// Parse untrusted GitHub metadata; only stable, correctly versioned releases
/// and same-repository HTTPS download URLs can reach the UI.
pub fn parse_release(value: &serde_json::Value) -> Result<Release> {
    ensure!(
        value["draft"].as_bool() == Some(false) && value["prerelease"].as_bool() == Some(false),
        "No stable published release"
    );
    let tag = value["tag_name"].as_str().context("Release tag missing")?;
    let version = parse_version(tag)?;
    ensure!(version.pre.is_empty(), "Pre-release is not a stable update");
    let url = value["html_url"].as_str().context("Release URL missing")?;
    ensure!(
        github_path(url).as_deref() == Some(&format!("/ohmycloud/vellio/releases/tag/{tag}")),
        "Unexpected release URL"
    );
    let assets = value["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|asset| {
            let name = asset["name"].as_str()?;
            let url = asset["browser_download_url"].as_str()?;
            (trusted_download(url, name)
                && github_path(url)?
                    .starts_with(&format!("/ohmycloud/vellio/releases/download/{tag}/")))
            .then(|| Asset {
                name: name.into(),
                url: url.into(),
            })
        })
        .collect();
    Ok(Release {
        version: version.to_string(),
        url: url.into(),
        notes: value["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(6000)
            .collect(),
        published: value["published_at"].as_str().unwrap_or("").into(),
        assets,
    })
}

/// A 404 is normal for a repository which has not published its first release.
/// Offline, rate-limit and malformed-response failures must not report "up to date".
pub fn fetch_latest() -> Result<Option<Release>> {
    fetch(API)
}
fn fetch(api: &str) -> Result<Option<Release>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("Vellio/{VERSION}"))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .get(api)
        .header("Accept", "application/vnd.github+json")
        .send()
        .context("Could not connect to GitHub; check your network and retry")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    ensure!(
        response.status() != reqwest::StatusCode::FORBIDDEN
            && response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS,
        "GitHub rate limit reached; try again later or open Releases"
    );
    ensure!(
        response.status().is_success(),
        "GitHub update check failed (HTTP {})",
        response.status()
    );
    let mut bytes = vec![];
    response.take(MAX_RESPONSE + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_RESPONSE,
        "GitHub response is too large"
    );
    parse_release(&serde_json::from_slice(&bytes).context("Invalid GitHub response")?).map(Some)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Cache {
    pub enabled: bool,
    pub checked_at: i64,
    pub failed: bool,
    pub release: Option<Release>,
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            enabled: true,
            checked_at: 0,
            failed: false,
            release: None,
        }
    }
}
impl Cache {
    pub fn load(path: &Path) -> Result<Self> {
        serde_json::from_value(crate::store::read_json(path, serde_json::json!({}))?)
            .context("Could not read update preferences")
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        crate::store::write_json(path, &serde_json::to_value(self)?)
    }
    pub fn due(&self, now: i64) -> bool {
        self.enabled
            && (self.checked_at == 0
                || now < self.checked_at
                || now.saturating_sub(self.checked_at) >= if self.failed { RETRY } else { DAY })
    }
    /// Keep a known release after a failed check, but never label it current.
    /// Call on the UI thread so toggling automatic checks during a request wins.
    pub fn complete(&mut self, result: &Result<Option<Release>>, now: i64) {
        self.checked_at = now;
        self.failed = result.is_err();
        if let Ok(release) = result {
            self.release = release.clone();
        }
    }
    pub fn status(&self, current: &str) -> String {
        if self.failed {
            "Could not check for updates. Check your connection and try again.".into()
        } else if self.checked_at == 0 {
            "Not checked yet".into()
        } else if let Some(release) = &self.release {
            if release.is_newer_than(current) {
                format!("Version {} is available", release.version)
            } else {
                "You are up to date".into()
            }
        } else {
            "No public releases yet".into()
        }
    }
}
/// Machine-local, outside the movable/cloud-synced document library.
pub fn state_path() -> std::path::PathBuf {
    crate::settings::location_file().with_file_name("updates.json")
}
pub fn platform_label(os: &str, arch: &str) -> String {
    let os = match os {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        _ => os,
    };
    let arch = match (os, arch) {
        ("macOS", "aarch64") => "Apple Silicon",
        (_, "aarch64") => "ARM64",
        (_, "x86_64") => "x64",
        _ => arch,
    };
    format!("{os} · {arch}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str) -> serde_json::Value {
        json!({"tag_name":tag,"draft":false,"prerelease":false,
            "html_url":format!("{RELEASES}/tag/{tag}"),"body":"Release notes", "assets":[]})
    }
    #[test]
    fn semantic_versions_do_not_compare_as_strings_or_downgrade() {
        let latest = parse_release(&release("v1.10.0")).unwrap();
        assert!(latest.is_newer_than("1.9.9"));
        assert!(!latest.is_newer_than("2.0.0"));
        assert!(!latest.is_newer_than("1.10.0+other-build"));
        assert!(latest.is_newer_than("1.10.0-rc.1"));
        assert!(!latest.is_newer_than("bad-version"));
        assert!(parse_release(&release("v1.11.0-beta.1")).is_err());
        assert!(parse_release(&release("not-a-version")).is_err());
        for field in ["draft", "prerelease"] {
            let mut value = release("v1.0.0");
            value[field] = json!(true);
            assert!(parse_release(&value).is_err());
        }
    }
    #[test]
    fn selects_only_the_current_os_and_architecture_and_trusted_urls() {
        let mut value = release("v2.0.0");
        for (os, arch, ext) in [
            ("macos", "aarch64", "dmg"),
            ("macos", "x86_64", "dmg"),
            ("windows", "x86_64", "zip"),
            ("windows", "aarch64", "exe"),
            ("linux", "x86_64", "tar.gz"),
            ("linux", "aarch64", "AppImage"),
        ] {
            let name = format!("Vellio-2.0.0-{os}-{arch}.{ext}");
            value["assets"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":name,
                "browser_download_url":format!("{RELEASES}/download/v2.0.0/{name}")}));
        }
        let parsed = parse_release(&value).unwrap();
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
            ("windows", "aarch64"),
            ("linux", "x86_64"),
            ("linux", "aarch64"),
        ] {
            assert!(
                parsed
                    .download(os, arch)
                    .unwrap()
                    .name
                    .contains(&format!("{os}-{arch}"))
            );
        }
        assert!(parsed.download("linux", "riscv64").is_none());
        for bad in [
            "http://github.com/ohmycloud/vellio/releases/tag/v2.0.0",
            "https://github.com.evil.test/ohmycloud/vellio/releases/tag/v2.0.0",
            "https://github.com/other/repo/releases/tag/v2.0.0",
        ] {
            let mut bad_value = value.clone();
            bad_value["html_url"] = json!(bad);
            assert!(parse_release(&bad_value).is_err());
        }
        for asset in value["assets"].as_array_mut().unwrap() {
            asset["browser_download_url"] = json!("https://evil.test/installer.exe");
        }
        assert!(
            parse_release(&value)
                .unwrap()
                .download("macos", "aarch64")
                .is_none()
        );
    }
    #[test]
    fn universal_mac_package_is_a_fallback_not_a_cross_platform_download() {
        let mut latest = parse_release(&release("v2.0.0")).unwrap();
        latest.assets.push(Asset {
            name: "Vellio-2.0.0-macos-universal.zip".into(),
            url: format!("{RELEASES}/download/v2.0.0/Vellio-2.0.0-macos-universal.zip"),
        });
        assert!(latest.download("macos", "aarch64").is_some());
        assert!(latest.download("windows", "x86_64").is_none());
    }
    #[test]
    fn daily_checks_opt_out_manual_results_and_failed_retry_are_persisted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("updates.json");
        let mut cache = Cache::load(&path).unwrap();
        assert!(cache.due(100));
        cache.complete(&Ok(Some(parse_release(&release("v2.0.0")).unwrap())), 100);
        assert!(!cache.due(100 + DAY - 1));
        assert!(cache.due(100 + DAY));
        cache.enabled = false;
        assert!(!cache.due(100 + DAY));
        cache.complete(&Err(anyhow::anyhow!("offline")), 200);
        assert!(cache.release.is_some());
        assert!(cache.failed);
        assert!(cache.status("1.0.0").starts_with("Could not check"));
        cache.enabled = true;
        assert!(!cache.due(200 + RETRY - 1));
        assert!(cache.due(200 + RETRY));
        cache.save(&path).unwrap();
        let restored = Cache::load(&path).unwrap();
        assert!(restored.failed);
        assert_eq!(restored.release.unwrap().version, "2.0.0");
        cache.complete(&Ok(None), 300);
        assert!(cache.release.is_none());
        assert_eq!(cache.status("1.0.0"), "No public releases yet");
        assert!(
            cache.due(299),
            "a clock rollback must not suppress checks forever"
        );
    }
    #[test]
    fn http_errors_and_no_release_are_distinct_and_headers_are_set() {
        use std::{io::Write, net::TcpListener};
        for (code, body, success) in [
            ("200 OK", release("v2.0.0").to_string(), true),
            ("404 Not Found", "{}".into(), true),
            ("403 Forbidden", "{}".into(), false),
            ("429 Too Many Requests", "{}".into(), false),
            ("500 Internal Server Error", "{}".into(), false),
            ("200 OK", "not json".into(), false),
            ("302 Found", "".into(), false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                assert!(request.contains("user-agent: vellio/"));
                assert!(request.contains("accept: application/vnd.github+json"));
                write!(
                    stream,
                    "HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            assert_eq!(fetch(&url).is_ok(), success, "{code}");
            server.join().unwrap();
        }
    }
}
