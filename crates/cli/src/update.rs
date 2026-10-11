//! `anneal self-update`, the course bundle behind `anneal course update`, and the once-a-day "something newer is available" hint
//! (docs/CLI_INSTALL.md). All of it asks the app you signed in to (`anneal course login <url>`), or the one given with `--from`.
//! HTTP goes through `curl`, as everywhere else in the CLI.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};

use crate::course_sync;
use crate::embedded;

/// How often the hint asks the app (the answer is remembered in between).
const HINT_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The name the app gives this computer's build (`linux-x64`, `macos-arm64`, `macos-x64`).
fn target_for(os: &str, arch: &str) -> Option<String> {
    let os = match os {
        "macos" => "macos",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match arch {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        _ => return None,
    };
    Some(format!("{os}-{arch}"))
}

fn target() -> anyhow::Result<String> {
    target_for(std::env::consts::OS, std::env::consts::ARCH).with_context(|| format!("there is no anneal build for {}/{} (macOS and Linux on arm64 or x86-64 only)", std::env::consts::OS, std::env::consts::ARCH))
}

/// The app to ask: `--from`, else the one you signed in to.
fn base_url(from: Option<&str>) -> Option<String> {
    from.map(str::to_owned).or_else(course_sync::app_url).map(|u| u.trim_end_matches('/').to_owned())
}

fn cache_dir() -> Option<PathBuf> {
    embedded::cache_base()
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

/// `curl` for a small JSON answer.
fn get_json(url: &str, secs: u32) -> anyhow::Result<serde_json::Value> {
    let out = Command::new("curl").args(["-fsS", "--max-time", &secs.to_string(), url]).output().context("running curl (is it installed?)")?;
    if !out.status.success() {
        bail!("{url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    serde_json::from_slice(&out.stdout).with_context(|| format!("{url} did not answer with JSON"))
}

/// The real path of the running `anneal`, symlinks resolved, so an update replaces the file and not a link to it.
fn this_binary() -> anyhow::Result<PathBuf> {
    fs::canonicalize(std::env::current_exe().context("finding the running anneal")?).context("finding the running anneal")
}

/// The app's build for this computer: (sha256, size).
fn latest_build(base: &str, target: &str, secs: u32) -> anyhow::Result<(String, u64)> {
    let v = get_json(&format!("{base}/cli/{target}/version"), secs)?;
    match (v["sha256"].as_str(), v["size"].as_u64()) {
        (Some(sha), Some(size)) => Ok((sha.to_owned(), size)),
        _ => bail!("the app's answer about the {target} build is not what this anneal expects"),
    }
}

/// `anneal self-update`: replace this binary with the app's build when they differ.
pub fn self_update(from: Option<String>, check: bool) -> anyhow::Result<ExitCode> {
    let base = base_url(from.as_deref()).context("sign in first (`anneal course login <url>`), or say where to look: `anneal self-update --from <url>`")?;
    let target = target()?;
    let exe = this_binary()?;
    let mine = sha256_file(&exe).with_context(|| format!("reading {}", exe.display()))?;
    let (sha, size) = latest_build(&base, &target, 20)?;
    if mine == sha {
        println!("anneal is up to date ({}).", &mine[..12]);
        return Ok(ExitCode::SUCCESS);
    }
    if check {
        println!("A newer anneal is available at {base} (yours {}, theirs {}). Run `anneal self-update`.", &mine[..12], &sha[..12]);
        return Ok(ExitCode::SUCCESS);
    }
    // Download beside the binary (the same file system, so the final rename is atomic), check it, then swap it in.
    let dir = exe.parent().context("the binary has no directory")?;
    let tmp = dir.join(format!(".anneal.update.{}", std::process::id()));
    let result = download_and_swap(&base, &target, &tmp, &exe, &sha, size);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    println!("Updated {} ({} -> {}).", exe.display(), &mine[..12], &sha[..12]);
    Ok(ExitCode::SUCCESS)
}

fn download_and_swap(base: &str, target: &str, tmp: &Path, exe: &Path, sha: &str, size: u64) -> anyhow::Result<()> {
    let url = format!("{base}/downloads/anneal-{target}");
    let status = Command::new("curl").args(["-fsSL", "--max-time", "600", &url, "-o"]).arg(tmp).status().context("running curl (is it installed?)")?;
    if !status.success() {
        bail!("could not download {url}");
    }
    let got = fs::metadata(tmp)?.len();
    if got != size {
        bail!("the download is {got} bytes, the app says {size}: try again");
    }
    if sha256_file(tmp)? != sha {
        bail!("the download does not match the app's checksum: try again");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(tmp, fs::Permissions::from_mode(0o755))?;
    }
    // A build that cannot even say its version is not one to leave in place of the working one.
    let said = Command::new(tmp).arg("--version").output().context("running the downloaded anneal")?;
    if !said.status.success() || !String::from_utf8_lossy(&said.stdout).starts_with("anneal") {
        bail!("the downloaded file is not a working anneal build");
    }
    fs::rename(tmp, exe).with_context(|| format!("replacing {} (is the directory writable? `curl -fsSL {base}/install.sh | sh` installs to ~/.local/bin)", exe.display()))
}

// ---- the course bundle ------------------------------------------------------------------------------------------------------------

/// What asking the app for a course's newest files came to.
pub enum Bundle {
    /// The course files this CLI already has are the newest.
    Current,
    /// The newest files, extracted in a `courses/` directory.
    Fresh(PathBuf),
    /// Could not ask or could not use the answer, and why; the files built into this CLI stay in use.
    Unavailable(String),
}

fn bundles_dir() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("bundles"))
}

/// The bundle this CLI would use now: the last one fetched (if it is still on disk), else the one built into the binary.
fn current_bundle(course: &str) -> Option<(String, Option<PathBuf>)> {
    if let Some(dir) = bundles_dir() {
        let pointer = fs::read_to_string(dir.join(format!("{course}.current"))).ok().map(|s| s.trim().to_owned());
        if let Some(fp) = pointer.filter(|f| f.len() == 16 && f.chars().all(|c| c.is_ascii_hexdigit())) {
            let at = dir.join(format!("{course}-{fp}"));
            if at.join(".complete").exists() {
                return Some((fp, Some(at)));
            }
        }
    }
    embedded::course_fingerprint(course).map(|f| (format!("{f:016x}"), None))
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    headers.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_owned()))
}

/// Asks the app for `course`'s newest files and, if they differ from the ones in use, extracts them under the cache.
pub fn fetch_bundle(course: &str, from: Option<&str>) -> Bundle {
    match try_fetch_bundle(course, from) {
        Ok(b) => b,
        Err(e) => Bundle::Unavailable(format!("{e:#}")),
    }
}

fn try_fetch_bundle(course: &str, from: Option<&str>) -> anyhow::Result<Bundle> {
    let base = base_url(from).context("not signed in: `anneal course login <url>`, or pass --from <url>")?;
    let dir = bundles_dir().context("no home directory for the course cache")?;
    fs::create_dir_all(&dir)?;
    let have = current_bundle(course);
    let scratch = dir.join(format!(".fetch-{}", std::process::id()));
    let (head, body) = (scratch.with_extension("head"), scratch.with_extension("body"));
    let cleanup = || {
        let _ = fs::remove_file(&head);
        let _ = fs::remove_file(&body);
        let _ = fs::remove_dir_all(&scratch);
    };
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "--max-time", "120", "--max-filesize", "67108864", "-w", "%{http_code}", "-D"]).arg(&head).arg("-o").arg(&body);
    if let Some((fp, _)) = &have {
        cmd.args(["-H", &format!("If-None-Match: \"{fp}\"")]);
    }
    cmd.arg(format!("{base}/courses-bundle/{course}"));
    let out = cmd.output().context("running curl (is it installed?)")?;
    let result = (|| -> anyhow::Result<Bundle> {
        if !out.status.success() {
            bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        match String::from_utf8_lossy(&out.stdout).trim() {
            "304" => Ok(match have {
                Some((_, Some(dir))) => Bundle::Fresh(dir),
                _ => Bundle::Current,
            }),
            "200" => {
                let headers = fs::read_to_string(&head).unwrap_or_default();
                let format: u32 = header_value(&headers, "x-anneal-bundle-format").and_then(|v| v.parse().ok()).unwrap_or(0);
                if format == 0 || format > anneal_content::bundle::FORMAT {
                    bail!("the app's course files are in a newer format than this anneal reads: run `anneal self-update` first");
                }
                let blob = fs::read(&body)?;
                let fp = format!("{:016x}", anneal_content::bundle::fingerprint(&blob));
                let at = dir.join(format!("{course}-{fp}"));
                if !at.join(".complete").exists() {
                    // Extract beside it and rename, so an interrupted run leaves no half-written directory in use.
                    embedded::extract(&blob, &scratch).context("the course files from the app are damaged")?;
                    let _ = fs::remove_dir_all(&at);
                    fs::rename(&scratch, &at)?;
                }
                fs::write(dir.join(format!("{course}.current")), &fp)?;
                Ok(Bundle::Fresh(at))
            }
            "404" => bail!("the app has no course {course:?}"),
            other => bail!("the app answered {other}"),
        }
    })();
    cleanup();
    result
}

// ---- the hint -----------------------------------------------------------------------------------------------------------------------

/// After a command: one line on stderr if the app has a newer anneal or newer course files. Asks the app at most once a day and
/// remembers the answer in between. Best effort on purpose: offline, signed out or an old app all stay silent, and
/// `ANNEAL_NO_UPDATE_CHECK=1` turns it off.
pub fn hint(course: &str) {
    if std::env::var_os("ANNEAL_NO_UPDATE_CHECK").is_some() {
        return;
    }
    let Some(base) = base_url(None) else { return };
    let Some(dir) = cache_dir() else { return };
    let file = dir.join("update-hint");
    let fresh = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < HINT_EVERY);
    let text = if fresh {
        fs::read_to_string(&file).unwrap_or_default()
    } else {
        let text = check_lines(&base, course).join("\n");
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(&file, &text);
        text
    };
    if !text.is_empty() {
        eprintln!("\n{text}");
    }
}

fn check_lines(base: &str, course: &str) -> Vec<String> {
    let mut lines = Vec::new();
    if let (Ok(target), Ok(exe)) = (target(), this_binary())
        && let (Ok((sha, _)), Ok(mine)) = (latest_build(base, &target, 3), sha256_file(&exe))
        && sha != mine
    {
        lines.push("A newer anneal is available: run `anneal self-update`.".to_owned());
    }
    if let (Ok(v), Some((have, _))) = (get_json(&format!("{base}/courses-bundle/{course}/version"), 3), current_bundle(course))
        && v["fingerprint"].as_str().is_some_and(|f| f != have)
    {
        lines.push("New course content is available: run `anneal course update`.".to_owned());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_computer_maps_to_the_apps_build_names() {
        assert_eq!(target_for("macos", "aarch64").as_deref(), Some("macos-arm64"));
        assert_eq!(target_for("macos", "x86_64").as_deref(), Some("macos-x64"));
        assert_eq!(target_for("linux", "x86_64").as_deref(), Some("linux-x64"));
        assert_eq!(target_for("windows", "x86_64"), None);
        assert_eq!(target_for("linux", "riscv64"), None);
    }

    #[test]
    fn headers_are_read_without_regard_to_case() {
        let head = "HTTP/1.1 200 OK\r\nETag: \"abc\"\r\nX-Anneal-Bundle-Format: 1\r\n\r\n";
        assert_eq!(header_value(head, "x-anneal-bundle-format").as_deref(), Some("1"));
        assert_eq!(header_value(head, "etag").as_deref(), Some("\"abc\""));
        assert_eq!(header_value(head, "x-missing"), None);
    }

    #[test]
    fn a_file_is_hashed_in_hex() {
        let dir = std::env::temp_dir().join(format!("anneal-update-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("f"), b"abc").unwrap();
        assert_eq!(sha256_file(&dir.join("f")).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = fs::remove_dir_all(&dir);
    }
}
