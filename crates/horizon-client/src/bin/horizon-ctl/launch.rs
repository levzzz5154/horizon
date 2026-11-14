//! `horizon-ctl launch`: find a Horizon to start and start it with a control
//! socket. Launcher installs run from the game home, where saves and options.txt
//! live. Native builds use their recorded base's prepared resources.
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use horizon_client::logging::{data_dir, game_home};
use serde_json::{Value, json};

#[cfg(windows)]
const CLIENT: &str = "horizon-client.exe";
#[cfg(not(windows))]
const CLIENT: &str = "horizon-client";

pub fn launcher_home() -> PathBuf {
    match std::env::var_os("HORIZON_LAUNCHER_HOME").filter(|h| !h.is_empty()) {
        Some(h) => PathBuf::from(h),
        None => data_dir().join("horizon-launcher"),
    }
}
/// `v2026.10.13.1` gives [2026, 10, 13, 1]; a pre-release suffix is ignored.
pub fn version_parts(tag: &str) -> Vec<u64> {
    tag.trim_start_matches('v')
        .split('-')
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Installed launcher versions, newest first.
pub fn installed(versions: &Path) -> Vec<String> {
    let mut tags: Vec<String> = std::fs::read_dir(versions)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join(".installed").exists() && e.path().join(CLIENT).exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    tags.sort_by_key(|t| std::cmp::Reverse(version_parts(t)));
    tags
}

/// A folder's prepared resources, as the launcher prepares them.
pub fn prepared_resources(dir: &Path) -> Option<PathBuf> {
    let version = std::fs::read_to_string(dir.join("research-lock.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v["minecraft"]["version"].as_str().map(str::to_owned))
        .unwrap_or_else(|| horizon_core::VANILLA_VERSION.into());
    let resources = dir.join(".local/horizon-client").join(version);
    resources
        .join("horizon-resources.json")
        .exists()
        .then_some(resources)
}

/// The same validated build record the launcher uses to locate a native build's base.
fn installed_base(dir: &Path) -> Result<Option<String>, String> {
    let path = dir.join("native-mods.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let json =
        serde_json::from_str::<Value>(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    horizon_mod_package::native_build::Record::from_json(&json)
        .map(|record| Some(record.base.to_owned()))
        .ok_or_else(|| {
            format!(
                "{} is not a valid native build record; rebuild it from an installed Horizon version",
                path.display()
            )
        })
}

#[derive(Debug, PartialEq)]
pub struct Install {
    pub game: PathBuf,
    /// Passed as --resources unless the caller gave their own.
    pub resources: Option<PathBuf>,
    /// The working directory (the game home for prepared installs).
    pub cwd: Option<PathBuf>,
    pub source: &'static str,
    pub version: Option<String>,
}

/// Which game to start: an explicit binary, the one beside horizon-ctl, or a
/// launcher install.
pub fn find(
    game: Option<&Path>,
    version: Option<&str>,
    resources: Option<&Path>,
    beside: Option<&Path>,
    launcher: &Path,
) -> Result<Install, String> {
    let versions = launcher.join("versions");
    let prepared = |dir: &Path, source, version: Option<String>| {
        let base = if resources.is_none() {
            installed_base(dir)?
        } else {
            None
        };
        let base_dir = base.as_ref().map(|base| versions.join(base));
        let automatic_resources = prepared_resources(base_dir.as_deref().unwrap_or(dir));
        let cwd = (version.is_some() || automatic_resources.is_some()).then(game_home);
        let resources = resources.map(Path::to_path_buf).or(automatic_resources);
        if resources.is_none()
            && let Some(tag) = version.as_deref().or(base.as_deref())
        {
            let prepare = base.as_deref().unwrap_or(tag);
            return Err(format!(
                "{tag} is installed but not prepared; prepare its resources first (horizon-launcher prepare --version {prepare})"
            ));
        }
        Ok(Install {
            game: dir.join(CLIENT),
            cwd,
            resources,
            source,
            version,
        })
    };
    if let Some(game) = game {
        if !game.is_file() {
            return Err(format!("{} is not a file", game.display()));
        }
        let dir = game.parent().unwrap_or(Path::new("."));
        let mut install = prepared(dir, "--game", None)?;
        install.game = game.to_owned();
        return Ok(install);
    }
    if version.is_none()
        && let Some(dir) = beside
        && dir.join(CLIENT).is_file()
    {
        return prepared(dir, "beside horizon-ctl", None);
    }
    let tags = installed(&versions);
    let tag = match version {
        Some(v) => {
            let want = if v.starts_with('v') { v.to_owned() } else { format!("v{v}") };
            tags.iter()
                .find(|t| **t == want || **t == v)
                .cloned()
                .ok_or_else(|| format!("version {v} is not installed in {} (installed: {tags:?})", versions.display()))?
        }
        None => tags.first().cloned().ok_or_else(|| {
            format!(
                "no Horizon found: no horizon-client beside horizon-ctl and no version installed by the launcher in {}. Install one with the launcher (horizon-launcher install latest) or pass --game PATH.",
                versions.display()
            )
        })?,
    };
    prepared(&versions.join(&tag), "launcher", Some(tag))
}

pub struct Options {
    pub game: Option<PathBuf>,
    pub version: Option<String>,
    pub resources: Option<PathBuf>,
    pub wait: bool,
    pub game_args: Vec<String>,
}

pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        game: None,
        version: None,
        wait: false,
        resources: None,
        game_args: Vec::new(),
    };
    let mut i = 0;
    while i < args.len() {
        let value = |i: usize| {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("{} needs a value", args[i]))
        };
        match args[i].as_str() {
            "--game" => {
                o.game = Some(value(i)?.into());
                i += 1;
            }
            "--version" => {
                o.version = Some(value(i)?);
                i += 1;
            }
            "--resources" => {
                o.resources = Some(value(i)?.into());
                i += 1;
            }
            "--wait" | "--wait-ready" => o.wait = true,
            other => o.game_args.push(other.to_owned()),
        }
        i += 1;
    }
    Ok(o)
}

/// Start the game and wait for its control endpoint; Ok(reply) or Err(reply).
pub fn launch(socket: &Path, args: &[String]) -> Result<Value, Value> {
    let o = parse(args).map_err(|e| json!({ "error": e, "kind": "usage" }))?;
    horizon_client::control::check_path(socket).map_err(
        |e| json!({ "error": format!("--socket {}: {e}", socket.display()), "kind": "usage" }),
    )?;
    if horizon_client::control::connect(socket).is_ok() {
        return Err(json!({
            "error": format!("a game is already listening on {}; quit it first or pass another --socket", socket.display()),
            "kind": "connection",
        }));
    }
    let exe = std::env::current_exe().ok();
    let beside = exe.as_deref().and_then(Path::parent);
    let install = find(
        o.game.as_deref(),
        o.version.as_deref(),
        o.resources.as_deref(),
        beside,
        &launcher_home(),
    )
    .map_err(|e| json!({ "error": e, "kind": "launch" }))?;
    horizon_client::control::prepare_path(socket).map_err(
        |e| json!({ "error": format!("--socket {}: {e}", socket.display()), "kind": "launch" }),
    )?;
    let log = socket.with_extension("log");
    let file = std::fs::File::create(&log)
        .map_err(|e| json!({ "error": format!("{}: {e}", log.display()), "kind": "launch" }))?;
    let mut cmd = Command::new(&install.game);
    if let Some(resources) = &install.resources {
        cmd.arg("--resources").arg(resources);
    }
    if let Some(cwd) = &install.cwd {
        let _ = std::fs::create_dir_all(cwd);
        cmd.current_dir(cwd);
    }
    let stderr = file
        .try_clone()
        .map_err(|e| json!({ "error": e.to_string(), "kind": "launch" }))?;
    // Windows: the game would otherwise hold our caller's pipes open (horizon-threads).
    #[cfg(windows)]
    horizon_threads::keep_stdio_from_children();
    let mut child = cmd
        .args(&o.game_args)
        .arg("--control-socket")
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(stderr)
        .spawn()
        .map_err(
            |e| json!({ "error": format!("{}: {e}", install.game.display()), "kind": "launch" }),
        )?;
    let deadline = Instant::now() + Duration::from_secs(300);
    let info = json!({
        "pid": child.id(),
        "socket": socket,
        "log": log,
        "game": install.game,
        "source": install.source,
        "version": install.version,
        "resources": install.resources,
        "home": install.cwd,
    });
    while Instant::now() < deadline {
        if horizon_client::control::connect(socket).is_ok() {
            let mut reply = json!({ "ok": true });
            reply
                .as_object_mut()
                .unwrap()
                .extend(info.as_object().unwrap().clone());
            return Ok(reply);
        }
        if let Ok(Some(status)) = child.try_wait() {
            let tail = std::fs::read_to_string(&log).unwrap_or_default();
            let tail: Vec<&str> = tail.lines().rev().take(15).collect();
            return Err(json!({
                "error": format!("the game exited ({status}) before opening the control socket"),
                "kind": "launch",
                "log_tail": tail.into_iter().rev().collect::<Vec<_>>(),
                "log": log,
            }));
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(
        json!({ "error": "control socket did not open within 300 seconds", "kind": "launch", "log": log }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Refused up front: a usage error, with no game started and no log beside the socket.
    #[cfg(unix)]
    #[test]
    #[ignore = "daily"]
    fn launch_refuses_a_socket_path_too_long() {
        let dir = temp("long-socket");
        let socket = dir.join(format!("{}.sock", "x".repeat(150)));
        let reply = launch(&socket, &[]).unwrap_err();
        assert_eq!(reply["kind"], "usage", "{reply}");
        assert!(reply["error"].as_str().unwrap().contains("too long"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("horizon-ctl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn fake_version(versions: &Path, tag: &str, prepared: bool) -> PathBuf {
        let dir = versions.join(tag);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(CLIENT), "").unwrap();
        std::fs::write(dir.join(".installed"), "").unwrap();
        std::fs::write(
            dir.join("research-lock.json"),
            r#"{"minecraft":{"version":"26.3"}}"#,
        )
        .unwrap();
        if prepared {
            let res = dir.join(".local/horizon-client/26.3");
            std::fs::create_dir_all(&res).unwrap();
            std::fs::write(res.join("horizon-resources.json"), "{}").unwrap();
        }
        dir
    }

    #[test]
    fn versions_sort_newest_first() {
        let home = temp("sort");
        let versions = home.join("versions");
        for tag in ["v2026.9.30", "v2026.10.13", "v2026.10.13.1", "v2026.10.2"] {
            fake_version(&versions, tag, true);
        }
        assert_eq!(
            installed(&versions),
            ["v2026.10.13.1", "v2026.10.13", "v2026.10.2", "v2026.9.30"]
        );
        let install = find(None, None, None, None, &home).unwrap();
        assert_eq!(install.version.as_deref(), Some("v2026.10.13.1"));
        assert!(
            install
                .resources
                .unwrap()
                .ends_with(".local/horizon-client/26.3")
        );
        assert_eq!(install.cwd, Some(game_home()));
        let picked = find(None, Some("2026.10.2"), None, None, &home).unwrap();
        assert_eq!(picked.version.as_deref(), Some("v2026.10.2"));
        assert!(find(None, Some("v1.0"), None, None, &home).is_err());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_game_beside_ctl_wins_and_dev_builds_keep_the_directory() {
        let home = temp("beside");
        fake_version(&home.join("versions"), "v2026.10.13", true);
        let build = temp("build");
        std::fs::write(build.join(CLIENT), "").unwrap();
        let install = find(None, None, None, Some(&build), &home).unwrap();
        assert_eq!(install.source, "beside horizon-ctl");
        assert_eq!((install.resources, install.cwd), (None, None));
        // --version always means a launcher install.
        assert_eq!(
            find(None, Some("v2026.10.13"), None, Some(&build), &home)
                .unwrap()
                .source,
            "launcher"
        );
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(build);
    }

    #[test]
    fn unprepared_or_missing_installs_explain_themselves() {
        let home = temp("unprepared");
        assert!(
            find(None, None, None, None, &home)
                .unwrap_err()
                .contains("no Horizon found")
        );
        fake_version(&home.join("versions"), "v2026.10.13", false);
        assert!(
            find(None, None, None, None, &home)
                .unwrap_err()
                .contains("not prepared")
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn native_builds_use_the_recorded_bases_prepared_resources() {
        let home = temp("native-base");
        let versions = home.join("versions");
        let base = fake_version(&versions, "developer-base", true);
        // The build record, not a version-name convention, chooses the resources.
        let native = fake_version(&versions, "local-built-client", false);
        std::fs::write(
            native.join("native-mods.json"),
            json!({
                "base": "developer-base",
                "horizon": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "target": "client",
            })
            .to_string(),
        )
        .unwrap();
        let install = find(None, Some("local-built-client"), None, None, &home).unwrap();
        assert_eq!(install.game, native.join(CLIENT));
        assert_eq!(install.version.as_deref(), Some("local-built-client"));
        assert_eq!(install.resources, prepared_resources(&base));
        assert_eq!(install.cwd, Some(game_home()));
        let beside = find(None, None, None, Some(&native), &home).unwrap();
        assert_eq!(beside.resources, prepared_resources(&base));
        std::fs::remove_file(base.join(".local/horizon-client/26.3/horizon-resources.json"))
            .unwrap();
        let error = find(None, Some("local-built-client"), None, None, &home).unwrap_err();
        assert!(
            error.contains("horizon-launcher prepare --version developer-base"),
            "{error}"
        );
        assert!(
            !error.contains("prepare --version local-built-client"),
            "{error}"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn invalid_native_records_do_not_fall_back_to_unrelated_resources() {
        let home = temp("invalid-native");
        let dir = fake_version(&home.join("versions"), "native-build", true);
        std::fs::write(
            dir.join("native-mods.json"),
            json!({
                "base": "developer-base",
                "horizon": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "target": "invalid",
            })
            .to_string(),
        )
        .unwrap();
        let error = find(None, Some("native-build"), None, None, &home).unwrap_err();
        assert!(error.contains("not a valid native build record"), "{error}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn explicit_resources_remain_authoritative_for_unprepared_installs() {
        let home = temp("explicit-resources");
        fake_version(&home.join("versions"), "v2026.10.13", false);
        let resources = home.join("chosen-resources");
        let install = find(None, Some("v2026.10.13"), Some(&resources), None, &home).unwrap();
        assert_eq!(install.resources.as_deref(), Some(resources.as_path()));
        assert_eq!(install.cwd, Some(game_home()));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn launch_options_pass_the_rest_to_the_game() {
        let args: Vec<String> = [
            "--world",
            "A",
            "--version",
            "v1",
            "--wait",
            "--resources",
            "chosen",
            "--seed",
            "3",
        ]
        .map(String::from)
        .to_vec();
        let o = parse(&args).unwrap();
        assert_eq!(o.version.as_deref(), Some("v1"));
        assert_eq!(o.resources.as_deref(), Some(Path::new("chosen")));
        assert!(o.wait);
        assert_eq!(o.game_args, ["--world", "A", "--seed", "3"]);
        assert!(parse(&["--game".to_owned()]).is_err());
        assert!(parse(&["--resources".to_owned()]).is_err());
    }
}
