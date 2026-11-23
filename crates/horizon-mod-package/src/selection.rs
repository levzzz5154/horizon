//! The package candidates the game loads, before content validation and conflicts.
mod context;
pub use context::{Argument, LaunchArguments, MissingValue, ModSelectionContext, data_dir_with};

use crate::{
    Package, find,
    mod_list::{DIR, NATIVE_DIR},
};
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

/// A selected package, read and validated once at the filesystem boundary.
#[derive(Clone)]
pub struct SelectedPackage {
    pub path: PathBuf,
    pub package: Arc<Package>,
}

/// The full package checks made for one launch, shared by code selection,
/// resource admission and the installed-mod list. Explicit live reads do not use it.
pub struct StartupPackages {
    reads: HashMap<PathBuf, Result<Arc<Package>, String>>,
}

impl StartupPackages {
    pub fn read(paths: &[PathBuf]) -> Self {
        let mut reads = HashMap::with_capacity(paths.len());
        for path in paths {
            reads
                .entry(path.clone())
                .or_insert_with_key(|path| Package::read(path).map(Arc::new));
        }
        Self { reads }
    }

    /// `None` means the path wasn't in this launch's scan. A refusal is retained.
    pub fn get(&self, path: &Path) -> Option<Result<&Arc<Package>, &str>> {
        self.reads
            .get(path)
            .map(|read| read.as_ref().map_err(String::as_str))
    }

    /// Reuse the exact error from the first validation. Only the manifest's
    /// identity is read for diagnostics, never the failed package's contents again.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn errors<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> impl Iterator<Item = (String, String)> + 'a {
        paths.iter().filter_map(|path| {
            let error = self.get(path)?.err()?;
            Some((refused_id(path)?, error.to_owned()))
        })
    }
}

/// Selected content-only packages, including explicit development paths.
/// Native packages are not content candidates, even through a development path:
/// their canonical source and compiled identity are owned by the engine.
pub fn content_packages<'a>(
    game_dir: &Path,
    enabled: &'a BTreeSet<String>,
    dev: &'a [PathBuf],
) -> impl Iterator<Item = SelectedPackage> + 'a {
    content_packages_with_snapshot(game_dir, enabled, dev, None)
}

/// The same selection using the checks already made by the startup loader,
/// where available. A launch uses one result for every path, including refusals.
pub fn content_packages_with_snapshot<'a>(
    game_dir: &Path,
    enabled: &'a BTreeSet<String>,
    dev: &'a [PathBuf],
    startup: Option<&'a StartupPackages>,
) -> impl Iterator<Item = SelectedPackage> + 'a {
    let mut paths = find(&game_dir.join(DIR));
    paths.extend(find(&game_dir.join(NATIVE_DIR)));
    paths.retain(|path| !dev.contains(path));
    paths.extend(dev.iter().cloned());
    paths.into_iter().filter_map(move |path| {
        let package = match startup {
            Some(reads) => reads.get(&path)?.ok()?.clone(),
            None => Arc::new(Package::read(&path).ok()?),
        };
        let selected =
            !package.is_native() && (enabled.contains(&package.manifest.id) || dev.contains(&path));
        selected.then_some(SelectedPackage { path, package })
    })
}

/// Retains invalid package diagnostics and manifest identities for loader refusals.
#[cfg(not(target_arch = "wasm32"))]
pub fn package_errors(paths: impl IntoIterator<Item = PathBuf>) -> Vec<(String, String)> {
    paths
        .into_iter()
        .filter_map(|path| {
            let error = Package::read(&path).err()?;
            Some((refused_id(&path)?, error))
        })
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn refused_id(path: &Path) -> Option<String> {
    use std::io::Read;

    let limit = crate::manifest::MAX_MANIFEST + 1;
    let mut text = String::new();
    if path.is_dir() {
        std::fs::File::open(path.join(crate::manifest::MANIFEST))
            .ok()?
            .take(limit)
            .read_to_string(&mut text)
            .ok()?;
    } else {
        let mut zip = zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()?;
        zip.by_name(crate::manifest::MANIFEST)
            .ok()?
            .take(limit)
            .read_to_string(&mut text)
            .ok()?;
    }
    if text.len() as u64 > crate::manifest::MAX_MANIFEST {
        return None;
    }
    // Invalid manifests still need an identity for the player-visible refusal.
    let manifest: toml::Table = toml::from_str(&text).ok()?;
    let id = manifest.get("id")?.as_str()?;
    crate::manifest::valid_mod_id(id).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let home = std::env::temp_dir()
                .join(format!("package-selection-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&home).unwrap();
            Self(home)
        }
        fn package(&self, folder: &str, id: &str, code: bool) -> PathBuf {
            let mut manifest = format!(
                "id='{id}'\nname='{id}'\nversion='1'\napi='0.2'\ncapabilities=['registries']\n"
            );
            let source: &[(&str, &[u8])] = if code {
                manifest += &format!(
                    "[native]\ngame='26.3'\nhorizon='{}'\nmixins='{}'\n",
                    "a".repeat(40),
                    "b".repeat(40)
                );
                &[
                    ("Cargo.toml", b"[package]\nname='example'\nversion='1'\n"),
                    ("src/lib.rs", b"pub fn hooks() {}"),
                ]
            } else {
                &[]
            };
            let directory = self.0.join(folder);
            fs::create_dir_all(&directory).unwrap();
            let path = directory.join(format!("{id}.hmod"));
            fs::write(&path, Package::write_files(&manifest, source).unwrap()).unwrap();
            path
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn content_toggles_apply_in_both_folders() {
        let home = Scratch::new();
        let first = home.package("horizon-mods", "first", false);
        let second = home.package("horizon-native-mods", "second", false);
        home.package("horizon-mods", "disabled", false);
        home.package("horizon-native-mods", "also_disabled", false);
        let enabled = BTreeSet::from(["first".into(), "second".into()]);
        let selected: Vec<_> = content_packages(&home.0, &enabled, &[])
            .map(|p| p.path)
            .collect();
        assert_eq!(selected, [first, second]);
    }

    #[test]
    fn content_candidates_do_not_load_native_inputs_even_when_enabled_or_developed() {
        let home = Scratch::new();
        let native = home.package("horizon-mods", "native", true);
        home.package("horizon-native-mods", "other_native", true);
        let plain = home.package("horizon-native-mods", "plain", false);
        let external = home.package("workspace", "external", false);
        let enabled = BTreeSet::from(["native".into(), "other_native".into(), "plain".into()]);
        let selected: Vec<_> = content_packages(&home.0, &enabled, &[native, external.clone()])
            .map(|p| p.path)
            .collect();
        assert_eq!(selected, [plain, external]);
    }

    #[test]
    fn development_paths_are_appended_once_and_invalid_packages_are_skipped() {
        let home = Scratch::new();
        let discovered = home.package("horizon-mods", "discovered", false);
        let external = home.package("workspace", "external", false);
        let invalid = home.0.join("horizon-mods/invalid.hmod");
        fs::write(&invalid, b"not a package").unwrap();
        let enabled = BTreeSet::from(["discovered".into(), "invalid".into()]);
        let dev = [discovered.clone(), external.clone(), invalid];
        let selected: Vec<_> = content_packages(&home.0, &enabled, &dev)
            .map(|p| p.path)
            .collect();
        assert_eq!(selected, [discovered, external]);
    }

    #[test]
    fn a_launch_keeps_the_checked_packages_while_live_selection_reads_current_files() {
        let home = Scratch::new();
        let good = home.0.join("good");
        let bad = home.0.join("bad");
        let write = |dir: &Path, id: &str, version: &str| {
            fs::create_dir_all(dir).unwrap();
            fs::write(
                dir.join(crate::manifest::MANIFEST),
                format!("id='{id}'\nname='{id}'\nversion={version}\napi='0.2'\ncapabilities=['registries']\n"),
            )
            .unwrap();
        };
        write(&good, "good", "'1'");
        write(&bad, "bad", "[]");
        let dev = [good.clone(), bad.clone()];
        let startup = StartupPackages::read(&dev);
        write(&good, "good", "'2'");
        write(&bad, "bad", "'1'");
        let enabled = BTreeSet::new();
        let boot = content_packages_with_snapshot(&home.0, &enabled, &dev, Some(&startup))
            .map(|p| {
                (
                    p.package.manifest.id.clone(),
                    p.package.manifest.version.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(boot, [("good".to_owned(), "1".to_owned())]);
        let live = content_packages(&home.0, &enabled, &dev)
            .map(|p| {
                (
                    p.package.manifest.id.clone(),
                    p.package.manifest.version.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            live,
            [
                ("good".to_owned(), "2".to_owned()),
                ("bad".to_owned(), "1".to_owned()),
            ],
        );
    }
}
