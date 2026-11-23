use horizon_launcher::browser::{installed::Installed, GAME};
use horizon_platform::mod_package::index::{self, Index, Kind, Project, Version};
use std::{fs, io::Write, path::Path, process::Command};

fn release(root: &Path, name: &str, number: &str, api: &str, files: &[(&str, &str)]) -> Version {
    let path = root.join(name);
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for &(name, text) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    let bytes = fs::read(&path).unwrap();
    let url_path = path.to_string_lossy().replace('\\', "/");
    Version {
        version: number.into(),
        published: String::new(),
        changelog: String::new(),
        api: api.into(),
        side: None,
        capabilities: Default::default(),
        game: GAME.into(),
        requires: Vec::new(),
        native_binaries: Vec::new(),
        file: index::File {
            name: name.into(),
            url: format!("file:///{}", url_path.trim_start_matches('/')),
            size: bytes.len() as u64,
            sha256: index::sha256(&bytes),
            signature: None,
        },
    }
}

#[test]
fn bulk_update_continues_after_a_native_compatibility_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let launcher = root.join("launcher");
    let game = root.join("game");
    let index_path = root.join("index.json");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_horizon-launcher"))
            .arg("mods")
            .args(args)
            .current_dir(root)
            .env("HORIZON_HOME", &game)
            .env("HORIZON_LAUNCHER_HOME", &launcher)
            .env("HORIZON_MOD_INDEX", &index_path)
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("HORIZON_LAUNCHER_TRASH", root.join("trash"))
            .env("HORIZON_LAUNCHER_CLIPBOARD", root.join("clipboard"))
            .env("HORIZON_LAUNCHER_DESKTOP", root.join("desktop"))
            .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
            .env("HORIZON_NO_AUDIO", "1")
            .output()
            .unwrap()
    };
    let [a, b, mixins] = ["a", "b", "f"].map(|digit| digit.repeat(40));
    for (tag, revision) in [("native-a", &a), ("native-b", &b)] {
        let folder = launcher.join("versions").join(tag);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(".installed"), "").unwrap();
        fs::write(
            folder.join("native-mods.json"),
            serde_json::json!({"base": "v0.0.1", "horizon": revision, "target": "client"})
                .to_string(),
        )
        .unwrap();
    }
    let pin = |tag: &str| {
        fs::write(
            launcher.join("instances.json"),
            serde_json::json!({"instances": {"horizon": {"version": tag}}}).to_string(),
        )
        .unwrap();
    };
    fs::write(launcher.join("settings.json"), r#"{"language":"en"}"#).unwrap();
    pin("native-a");

    let native = |number: &str| {
        release(
            root,
            &format!("coords_hud-{number}.hmod"),
            number,
            &format!("native-1/{a}/{mixins}"),
            &[
                (
                    "Cargo.toml",
                    &format!("[package]\nname='coords_hud'\nversion='{number}'\n"),
                ),
                ("src/lib.rs", "fn hook() {}"),
                (
                    "horizon-mod.toml",
                    &format!("id='coords_hud'\nname='Coordinates HUD'\nversion='{number}'\napi='0.2'\ncapabilities=['registries']\n[native]\ngame='{GAME}'\nhorizon='{a}'\nmixins='{mixins}'\n"),
                ),
                ("assets/coords_hud/sounds.json", "{}"),
            ],
        )
    };
    let pack = |number: &str| {
        release(
            root,
            &format!("lanterns-{number}.zip"),
            number,
            "94",
            &[
                (
                    "pack.mcmeta",
                    r#"{"pack":{"description":"Lanterns","pack_format":94}}"#,
                ),
                (
                    "assets/minecraft/lang/en_us.json",
                    &format!(r#"{{"block.minecraft.lantern":"Lanterns v{number}"}}"#),
                ),
            ],
        )
    };
    let mut index = Index::parse(include_str!(
        "../../crates/horizon-mod-package/tests/index/index-v1.json"
    ))
    .unwrap();
    let mut native_project = index.project("coords_hud").unwrap().clone();
    native_project.kind = Kind::NativeMod;
    native_project.key = None;
    native_project.versions = vec![native("0.1.0")];
    index.projects = vec![
        native_project.clone(),
        Project {
            id: "lanterns".into(),
            kind: Kind::ResourcePack,
            name: "Lanterns".into(),
            versions: vec![pack("1.0.0")],
            ..native_project
        },
    ];
    let publish =
        |index: &Index| fs::write(&index_path, serde_json::to_vec(index).unwrap()).unwrap();
    publish(&index);
    for id in ["coords_hud", "lanterns"] {
        let output = run(&[
            "install",
            id,
            "--source",
            "horizon",
            "--instance",
            "Horizon",
        ]);
        assert!(output.status.success(), "{output:?}");
    }
    let initial = Installed::load(&game).entries;
    let [native_before, pack_before] = initial.as_slice() else {
        panic!("expected both installs in order: {initial:?}");
    };
    assert_eq!(native_before.project, "coords_hud");
    assert_eq!(pack_before.project, "lanterns");
    let native_path = game.join(&native_before.file);
    let native_bytes = fs::read(&native_path).unwrap();

    // The native releases still require the old build; the resource pack can update on either.
    index.projects[0].versions.insert(0, native("0.2.0"));
    index.projects[1].versions.insert(0, pack("2.0.0"));
    publish(&index);
    pin("native-b");

    let single = run(&["update", "coords_hud", "--instance", "Horizon"]);
    assert_eq!(single.status.code(), Some(1), "{single:?}");
    assert!(
        String::from_utf8_lossy(&single.stderr)
            .contains("error: No published version of Coordinates HUD"),
        "{single:?}"
    );
    assert_eq!(Installed::load(&game).entries, initial);

    let preview = run(&["update", "--check", "--instance", "Horizon"]);
    assert_eq!(preview.status.code(), Some(1), "{preview:?}");
    assert!(
        String::from_utf8_lossy(&preview.stdout).contains("Lanterns 1.0.0: 2.0.0 is available."),
        "{preview:?}"
    );
    assert_eq!(Installed::load(&game).entries, initial);

    let output = run(&["update", "--instance", "Horizon"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in [
        "No published version of Coordinates HUD is made for the Horizon you play.",
        "Ask its author for an update made for this game.",
        "error: Native mods skipped for compatibility: 1.",
    ] {
        assert!(stderr.contains(line), "{output:?}");
    }
    assert!(String::from_utf8_lossy(&output.stdout).contains("Lanterns 2.0.0"));
    let updated = Installed::load(&game);
    assert_eq!(updated.lookup("coords_hud"), Some(native_before));
    assert_eq!(fs::read(&native_path).unwrap(), native_bytes);
    assert!(!game.join(&pack_before.file).exists());
    let pack_after = updated.lookup("lanterns").unwrap();
    assert_eq!(pack_after.version, "2.0.0");
    assert_eq!(
        fs::read(game.join(&pack_after.file)).unwrap(),
        fs::read(root.join("lanterns-2.0.0.zip")).unwrap()
    );
}
