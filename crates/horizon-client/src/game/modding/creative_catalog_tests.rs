//! Exercise the real creative catalog game dispatch with the same native and compiled guest code.
use std::{env, fs, path::PathBuf};

use super::{
    api_values,
    execution_role::ExecutionRole,
    mods::{self, ModsState},
    native_entity_refs::tests::{run_fresh_cases, with_game_role},
};
use horizon_mod_package::{
    Package,
    selection::{LaunchArguments, SelectedPackage},
};
use mod_api::{
    BroadcastMessageEvent, BuildCreativeModeTabContentsEvent, ClientHost, ClientTickPreEvent,
    EditCreativeModeTabContentsEvent, Flow, Hooks, Host, ModEntry, PackageInfo, PackageKind,
    ServerHost, ServerStartedEvent, Side,
};

#[path = "../../../../horizon-mod-wasm/tests/fixture/src/creative_catalog.rs"]
mod fixture;

struct Native;

#[mod_api::__subscriptions]
impl Hooks for Native {
    fn edit_creative_mode_tab_contents(
        &self,
        host: &dyn Host,
        event: &mut EditCreativeModeTabContentsEvent<'_>,
    ) {
        fixture::edit(host, event);
    }

    fn build_creative_mode_tab_contents(
        &self,
        host: &mut dyn ClientHost,
        event: &mut BuildCreativeModeTabContentsEvent<'_>,
    ) {
        fixture::legacy(host, event);
    }

    fn server_started(&self, host: &mut dyn ServerHost, _: &mut ServerStartedEvent) {
        fixture::server_started(host);
    }

    fn client_tick_pre(&self, host: &mut dyn ClientHost, _: &mut ClientTickPreEvent) {
        fixture::client_tick(host);
    }

    fn broadcast_message(&self, host: &dyn Host, event: &mut BroadcastMessageEvent<'_>) -> Flow {
        fixture::acknowledge(host, event)
    }
}

#[test]
#[ignore = "daily"]
fn native_creative_catalog_game_dispatch() {
    run(
        "game::creative_catalog_tests::native_creative_catalog_game_dispatch",
        false,
    );
}

#[test]
#[ignore = "requires prepared resources and compiled HORIZON_WASM_FIXTURE/HORIZON_WASM_STD"]
fn compiled_wasm_creative_catalog_game_dispatch() {
    run(
        "game::creative_catalog_tests::compiled_wasm_creative_catalog_game_dispatch",
        true,
    );
}

fn run(test: &str, wasm: bool) {
    const CHILD: &str = "HORIZON_CREATIVE_CATALOG_CHILD";
    // Both the current logical side and the process-owned registries must start fresh.
    if run_fresh_cases(test, CHILD, &["client", "dedicated"]) {
        return;
    }
    let role = match env::var(CHILD).unwrap().as_str() {
        "client" => ExecutionRole::Client,
        "dedicated" => ExecutionRole::DedicatedServer,
        role => panic!("unexpected creative fixture role: {role}"),
    };
    if wasm {
        horizon_guest_stack::run_on_guest_stack(move || dispatch(role, true));
    } else {
        dispatch(role, false);
    }
}

fn package_info(id: &str, version: &str, kind: PackageKind) -> PackageInfo {
    PackageInfo::new(
        id.into(),
        version.into(),
        kind,
        Side::Both,
        vec![id.into()],
        vec![],
    )
}

fn dispatch(role: ExecutionRole, wasm: bool) {
    let cwd = env::current_dir().unwrap();
    let cli = (!role.is_client()).then_some("--dedicated");
    let launch =
        LaunchArguments::parse(cli.into_iter().map(str::to_owned), &cwd, &cwd, None).unwrap();
    crate::entry::runtime_environment::capture(&launch);

    let scratch = tempfile::tempdir().unwrap();
    let selected = creative_package(scratch.path());
    let content = package_info(fixture::PACKAGE, "1.0.0", PackageKind::Content);
    let mut code = package_info("native_creative_catalog_probe", "1.0.0", PackageKind::Code);
    let mut loaded = wasm.then(|| {
        let path = PathBuf::from(env::var_os("HORIZON_WASM_FIXTURE").unwrap());
        let manifest = Package::read(&path).unwrap().manifest;
        code = package_info(&manifest.id, &manifest.version, PackageKind::Code);
        let loaded = horizon_mod_wasm::load(
            &fs::read(env::var_os("HORIZON_WASM_STD").unwrap()).unwrap(),
            vec![horizon_mod_wasm::WasmPackage {
                manifest,
                module: Package::read_wasm(&path).unwrap(),
            }],
        );
        assert!(loaded.refused.is_empty(), "{:?}", loaded.refused);
        assert_eq!(loaded.mods.len(), 1);
        loaded.initialize(&[code.clone(), content.clone()]).unwrap();
        loaded
    });
    let entries = if let Some(loaded) = &mut loaded {
        std::mem::take(&mut loaded.mods)
    } else {
        vec![ModEntry::new(
            "native_creative_catalog_probe",
            Box::new(Native),
        )]
    };
    let entries: mod_api::Mods = entries.into();
    let base = PathBuf::from(env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    api_values::install_test_platform();
    mod_platform::__private::install_packages(&[code], &[content]);
    mods::install_registration_data(&base).unwrap();
    mod_platform::testing::with_mods(entries.clone(), || {
        api_values::install_parts(&entries).unwrap();
        mod_api::__private::mark_game_thread();
        let root = mods::content_root(
            &base,
            &ModsState {
                selected: vec![selected].into(),
                ..Default::default()
            },
        )
        .unwrap();
        with_game_role(root.prepared().to_path_buf(), role, |game| {
            let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
            if role.is_client() {
                game.refresh_creative_tabs();
                game.native_client_tick_pre();
            } else {
                // Use the dedicated resource role and the world-ready publication path. No GUI
                // or creative inventory is started on the server.
                game.native_lifecycle_open_started();
                game.native_lifecycle_world_ready();
                assert!(game.window.is_none() && game.renderer.is_none());
            }
            let mut line = horizon_client::text::Text::literal(fixture::PING);
            let mut cancelled = false;
            game.native_broadcast_message(&mut line, &mut cancelled);
            assert_eq!(
                line.plain(),
                fixture::ACK,
                "the dispatched consumer must finish"
            );
            assert!(!cancelled);
        });
    });
}

fn creative_package(scratch: &std::path::Path) -> SelectedPackage {
    let manifest = r#"
id = "creative_catalog_probe"
name = "Creative Catalog Probe"
version = "1.0.0"
api = "0.2"
side = "both"
capabilities = ["registries"]
"#;
    let files: &[(&str, &[u8])] = &[
        (
            "data/creative_catalog_probe/horizon/creative_tab/a_last.json",
            br#"{"title":"Last","icon":"minecraft:dirt","items":["minecraft:dirt"],"after":"creative_catalog_probe:m_specimens"}"#,
        ),
        (
            "data/creative_catalog_probe/horizon/creative_tab/m_specimens.json",
            br#"{"title":"Creative specimens","icon":{"id":"minecraft:stone","components":{"minecraft:custom_name":{"text":"Specimen"}}},"items":["minecraft:flint","minecraft:emerald","minecraft:stick"]}"#,
        ),
        (
            "data/creative_catalog_probe/horizon/creative_tab/z_first.json",
            br#"{"title":"First","icon":"minecraft:oak_log","items":["minecraft:oak_log"],"before":"creative_catalog_probe:m_specimens"}"#,
        ),
    ];
    let path = scratch.join("creative_catalog_probe.hmod");
    fs::write(&path, Package::write_files(manifest, files).unwrap()).unwrap();
    SelectedPackage {
        package: Package::read(&path).unwrap(),
        path,
    }
}
