//! Native debug renderers in a real game: when the renderer list refreshes, and what a frame's
//! world geometry holds as a renderer is switched and the world goes.
use super::{
    execution_role::ExecutionRole,
    mods,
    native_entity_refs::tests::{run_fresh_cases, with_client_game},
};
use glam::Mat4;
use horizon_client::{
    debug_entries::{self, Category, DebugEntries, Profile, Status},
    world_geometry::WorldGeometry,
};
use horizon_protocol::packets::play::{self, Clientbound as C};
use mod_api::{
    ClientHost, DebugEntry, DebugGroup, DebugRenderer, DebugRendererDrawEvent, Hooks, ModEntry,
    RegisterDebugEntriesEvent, RegisterDebugRenderersEvent, WorldDepth, WorldLine,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const CAMERA: [f64; 3] = [100.0, 64.0, 100.0];
const SHOWN: &str = "marker:pipes";
const OTHER: &str = "marker:wires";
/// Pushed as a renderer, but no entry declares it.
const UNDECLARED: &str = "marker:ghost";

/// The ids `debug_renderer_draw` was asked about since the last frame.
static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// What the mod pushes at a refresh.
static PUSHED: Mutex<Vec<&str>> = Mutex::new(Vec::new());
/// The refreshes since the last look (`refreshes`).
static REFRESHES: AtomicUsize = AtomicUsize::new(0);

struct Marker;
#[mod_api::__subscriptions]
impl Hooks for Marker {
    fn register_debug_entries(
        &self,
        _: &mod_api::RegistrationContext,
        event: &mut RegisterDebugEntriesEvent<'_>,
    ) {
        for id in [SHOWN, OTHER] {
            event.entries.push(DebugEntry {
                id: id.into(),
                group: DebugGroup::Renderer,
            });
        }
    }
    fn register_debug_renderers(
        &self,
        _: &mod_api::RegistrationContext,
        event: &mut RegisterDebugRenderersEvent<'_>,
    ) {
        REFRESHES.fetch_add(1, Ordering::Relaxed);
        for id in PUSHED.lock().unwrap().iter() {
            event.renderers.push(DebugRenderer {
                id: (*id).to_owned(),
            });
        }
    }
    fn debug_renderer_draw(&self, _: &dyn ClientHost, event: &mut DebugRendererDrawEvent<'_>) {
        if !matches!(event.id, SHOWN | OTHER) {
            return;
        }
        ASKED.lock().unwrap().push(event.id.to_owned());
        let marker = WorldLine::new(
            [100.25, 64.0, 99.0],
            [101.0, 64.0, 99.0],
            0xFF00FF00,
            2.0,
            WorldDepth::Tested,
        );
        event.draw.lines(&[marker]).unwrap();
    }
}

/// One frame's view: the lines it queued, camera relative, and the renderers asked.
fn frame(game: &mut super::Game<'_>, draw: &mut WorldGeometry) -> (Vec<[f32; 6]>, Vec<String>) {
    let title = game.shows_panorama();
    game.fill_world_geometry(title, CAMERA, Mat4::IDENTITY, 0.0, draw, &mut Vec::new());
    let lines = draw
        .batch()
        .lines
        .iter()
        .map(|l| std::array::from_fn(|i| l[i]));
    (lines.collect(), std::mem::take(&mut *ASKED.lock().unwrap()))
}

/// How many times the renderer list refreshed since the last call.
fn refreshes() -> usize {
    REFRESHES.swap(0, Ordering::Relaxed)
}

fn push(ids: &[&'static str]) {
    *PUSHED.lock().unwrap() = ids.to_vec();
}

#[test]
#[ignore = "daily"]
fn the_renderer_list_refreshes_with_the_selection_and_leaves_nothing_behind() {
    if fresh_process("the_renderer_list_refreshes_with_the_selection_and_leaves_nothing_behind") {
        return;
    }
    with_client_game(|game| {
        mod_platform::__private::install(vec![ModEntry::new("marker", Box::new(Marker))].into());
        push(&[SHOWN, OTHER, UNDECLARED]);
        mods::register_debug_entries(ExecutionRole::Client);
        // The options exist before any refresh, and pushing an id does not make one.
        assert_eq!(refreshes(), 0);
        for id in [SHOWN, OTHER] {
            assert_eq!(
                debug_entries::entry(id).map(|e| e.category),
                Some(Category::Renderer)
            );
        }
        // Not tied to a file: the statuses below stay out of `debug-profile.json`.
        game.debug_entries = DebugEntries::default();
        game.title = None;
        let mut draw = WorldGeometry::default();
        let none = (Vec::new(), Vec::new());

        // Off by default. The first view refreshes, as `lastDebugEntriesVersion` starts behind;
        // the next one does not.
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 1);
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 0);

        // Selected: one refresh, only that renderer is asked, and its marker is at the world
        // position.
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        let marker = vec![[0.25, 0.0, -1.0, 1.0, 0.0, -1.0]];
        let shown = (marker.clone(), vec![SHOWN.to_owned()]);
        assert_eq!(frame(game, &mut draw), shown);
        assert_eq!(refreshes(), 1);
        // A later frame replaces it rather than adding to it, with no refresh.
        assert_eq!(frame(game, &mut draw), shown);
        assert_eq!(refreshes(), 0);

        // The overlay changes the selection both ways.
        game.debug_entries.set_status(SHOWN, Status::InOverlay);
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 1);
        toggle_overlay(game);
        assert_eq!(frame(game, &mut draw), shown);
        assert_eq!(refreshes(), 1);
        toggle_overlay(game);
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 1);

        // So does reduced debug info, which hides a renderer.
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        assert_eq!(frame(game, &mut draw), shown);
        assert_eq!(refreshes(), 1);
        game.options_control(
            &["options-set", "reducedDebugInfo", "true"],
            "options-set reducedDebugInfo true",
        )
        .unwrap();
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 1);
        game.options_control(
            &["options-set", "reducedDebugInfo", "false"],
            "options-set reducedDebugInfo false",
        )
        .unwrap();
        assert_eq!(frame(game, &mut draw), shown);
        assert_eq!(refreshes(), 1);

        // A built-in profile leaves it out.
        game.debug_entries.load_profile(Profile::Default);
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 1);

        // What a mod stops pushing stays until the next refresh, then nothing of it is left. The
        // renderer with no entry is never asked.
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        game.debug_entries.set_status(OTHER, Status::AlwaysOn);
        let both = vec![SHOWN.to_owned(), OTHER.to_owned()];
        assert_eq!(frame(game, &mut draw).1, both);
        assert_eq!(refreshes(), 1);
        push(&[OTHER]);
        assert_eq!(frame(game, &mut draw).1, both);
        assert_eq!(refreshes(), 0);
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        assert_eq!(frame(game, &mut draw).1, [OTHER]);
        assert_eq!(refreshes(), 1);
        push(&[SHOWN]);

        // Disabled: the marker goes with the next frame.
        game.debug_entries.set_status(OTHER, Status::Never);
        game.debug_entries.set_status(SHOWN, Status::Never);
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 1);

        // Leaving the world with it on: the title frame asks nothing and shows nothing, and
        // refreshes nothing, as it has no `emitGizmos`.
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        assert_eq!(frame(game, &mut draw), shown);
        assert_eq!(refreshes(), 1);
        game.leave_server(None).unwrap();
        game.debug_entries.set_status(SHOWN, Status::Never);
        assert_eq!(frame(game, &mut draw), none);
        assert_eq!(refreshes(), 0);

        // `handleLogin` refreshes the list, with no change of selection.
        let (pipe, _server) = horizon_protocol::connection::Pipe::pair();
        game.connect_local(pipe, "Steve").unwrap();
        game.handle_packet(login()).unwrap();
        assert_eq!(refreshes(), 1);
        game.fill_world_geometry(
            false,
            CAMERA,
            Mat4::IDENTITY,
            0.0,
            &mut draw,
            &mut Vec::new(),
        );
        ASKED.lock().unwrap().clear();
        assert_eq!(refreshes(), 1, "first world view after login");
    });
}

// Mod declarations and the marker's state are process-global, not one Game's state.
fn fresh_process(name: &str) -> bool {
    run_fresh_cases(
        &format!("game::debug_renderer_tests::{name}"),
        "HORIZON_DEBUG_RENDERER_FIXTURE",
        &["run"],
    )
}

fn login() -> C {
    C::Login(play::Login {
        player_id: 7,
        hardcore: false,
        levels: vec!["minecraft:overworld".into()],
        max_players: 20,
        chunk_radius: 8,
        simulation_distance: 8,
        reduced_debug_info: false,
        show_death_screen: true,
        limited_crafting: false,
        spawn: play::SpawnInfo {
            dimension_type: 0,
            dimension: "minecraft:overworld".into(),
            seed: 1,
            game_type: 0,
            previous_game_type: None,
            debug: false,
            flat: false,
            last_death: None,
            portal_cooldown: 0,
            sea_level: 63,
        },
        online_mode: false,
        enforces_secure_chat: false,
    })
}

fn toggle_overlay(game: &mut super::Game<'_>) {
    use horizon_client::debug_keys::KeyAction;
    use winit::keyboard::KeyCode;
    game.debug_key(KeyCode::F3, KeyAction::Press);
    game.debug_key(KeyCode::F3, KeyAction::Release);
}

/// Prime a real world's view, change only the mod's renderer state, then invalidate through
/// the real input/packet path. The next view must collect the new list exactly once.
fn invalidation_refreshes(expected: &[&str], change: impl FnOnce(&mut super::Game<'_>)) {
    with_client_game(|game| {
        mod_platform::__private::install(vec![ModEntry::new("marker", Box::new(Marker))].into());
        mods::register_debug_entries(ExecutionRole::Client);
        game.debug_entries = DebugEntries::default();
        game.title = None;
        let (pipe, _server) = horizon_protocol::connection::Pipe::pair();
        game.connect_local(pipe, "Steve").unwrap();
        game.handle_packet(login()).unwrap();
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        game.debug_entries.set_status(OTHER, Status::AlwaysOn);
        let mut draw = WorldGeometry::default();
        push(&[SHOWN]);
        game.fill_world_geometry(
            false,
            CAMERA,
            Mat4::IDENTITY,
            0.0,
            &mut draw,
            &mut Vec::new(),
        );
        ASKED.lock().unwrap().clear();
        refreshes();
        push(&[OTHER]);
        change(game);
        game.fill_world_geometry(
            false,
            CAMERA,
            Mat4::IDENTITY,
            0.0,
            &mut draw,
            &mut Vec::new(),
        );
        assert_eq!(
            (refreshes(), std::mem::take(&mut *ASKED.lock().unwrap())),
            (1, expected.iter().map(|id| (*id).to_owned()).collect()),
            "next world view must replace the stale renderer list"
        );
        game.fill_world_geometry(
            false,
            CAMERA,
            Mat4::IDENTITY,
            0.0,
            &mut draw,
            &mut Vec::new(),
        );
        ASKED.lock().unwrap().clear();
        assert_eq!(refreshes(), 0, "no work on an unchanged frame");
    });
}

#[test]
#[ignore = "daily"]
fn debug_renderer_respawn_invalidates_unchanged_selection() {
    if fresh_process("debug_renderer_respawn_invalidates_unchanged_selection") {
        return;
    }
    invalidation_refreshes(&[OTHER], |game| {
        let C::Login(login) = login() else {
            unreachable!()
        };
        game.handle_packet(C::Respawn(play::Respawn {
            spawn: login.spawn,
            keep: 3,
        }))
        .unwrap();
    });
}

fn same_value_event(event: i8, expected: &[&str]) {
    invalidation_refreshes(expected, |game| {
        push(&[SHOWN]);
        game.handle_packet(C::EntityEvent(play::EntityEvent { id: 7, event }))
            .unwrap();
        let mut draw = WorldGeometry::default();
        game.fill_world_geometry(
            false,
            CAMERA,
            Mat4::IDENTITY,
            0.0,
            &mut draw,
            &mut Vec::new(),
        );
        ASKED.lock().unwrap().clear();
        refreshes();
        push(&[OTHER]);
        game.handle_packet(C::EntityEvent(play::EntityEvent { id: 7, event }))
            .unwrap();
    });
}

#[test]
#[ignore = "daily"]
fn debug_renderer_same_value_reduced_info_on_invalidates() {
    if fresh_process("debug_renderer_same_value_reduced_info_on_invalidates") {
        return;
    }
    same_value_event(22, &[]);
}

#[test]
#[ignore = "daily"]
fn debug_renderer_same_value_reduced_info_off_invalidates() {
    if fresh_process("debug_renderer_same_value_reduced_info_off_invalidates") {
        return;
    }
    same_value_event(23, &[OTHER]);
}

#[test]
#[ignore = "daily"]
fn debug_renderer_overlay_round_trip_invalidates() {
    if fresh_process("debug_renderer_overlay_round_trip_invalidates") {
        return;
    }
    invalidation_refreshes(&[OTHER], |game| {
        let before = game.debug;
        toggle_overlay(game);
        assert_ne!(game.debug, before);
        toggle_overlay(game);
        assert_eq!(game.debug, before);
    });
}

#[test]
#[ignore = "daily"]
fn debug_renderer_second_login_invalidates_first_world_view() {
    if fresh_process("debug_renderer_second_login_invalidates_first_world_view") {
        return;
    }
    invalidation_refreshes(&[OTHER], |game| {
        push(&[SHOWN]);
        game.handle_packet(login()).unwrap();
        assert_eq!(refreshes(), 1, "immediate handleLogin refresh");
        // A logging-in listener can update its state after the immediate refresh.
        push(&[OTHER]);
    });
}

#[test]
#[ignore = "daily"]
fn hitbox_replacement_keeps_other_gizmos_and_reads_the_client_view() {
    use horizon_client::{chunk::Chunk, items::ItemStack, motion::PlayerMotion};
    use horizon_core::geometry::Vec3;
    use mod_api::{DebugRendererViewEvent, ENTITY_HITBOX_RENDERER, EntityReach, Hand};
    use std::sync::Arc;

    const PIG: u32 = 40;
    const PEARL: u32 = 41;
    const BOAT: u32 = 42;
    const DRAGON: u32 = 50;
    const ALPHA: f64 = 0.5;
    const BLOCK: [i32; 3] = [100, 63, 100];

    struct Replace(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl Hooks for Replace {
        fn debug_renderer_view(
            &self,
            host: &dyn ClientHost,
            event: &mut DebugRendererViewEvent<'_>,
        ) {
            if !matches!(event.id, ENTITY_HITBOX_RENDERER | SHOWN) {
                return;
            }
            assert_eq!(event.view.camera, CAMERA);
            assert_eq!(event.view.partial_tick, ALPHA as f32);
            assert!(!event.control.builtin_cancelled());
            let entry = |id| {
                event
                    .entities
                    .iter()
                    .find(|entry| entry.entity.is_some_and(|handle| handle.id() == id))
                    .unwrap()
            };
            let pig = entry(PIG);
            let pig_ref = pig.entity.unwrap();
            assert_eq!(pig.kind.unwrap().id(host), "minecraft:pig");
            assert_eq!(pig.position, [105.0, 64.0, 99.0]);
            assert_eq!(pig_ref.position(host), Some([106.0, 64.0, 99.0]));
            assert_eq!(pig.bounds[0][0], 105.0 - f64::from(0.9_f32) / 2.0);
            assert_eq!(pig.eye.unwrap()[1], 64.0 + pig.eye_height);
            assert_eq!(pig.look, Some([0.0, 0.0, 1.0]));
            assert_eq!(pig.seat, Some((entry(BOAT).entity.unwrap(), pig.position)));
            let pearl = entry(PEARL);
            assert!(pearl.projectile);
            assert_eq!(pearl.kind.unwrap().id(host), "minecraft:ender_pearl");
            assert_eq!(
                pearl.projectile_owner_kind.unwrap().id(host),
                "minecraft:player"
            );
            let dragon = entry(DRAGON).entity.unwrap();
            for id in DRAGON + 1..=DRAGON + 8 {
                let part = entry(id);
                assert_eq!(part.part_of, Some(dragon));
                assert_eq!(part.entity.unwrap().bounding_box(host), Some(part.bounds));
            }

            let player = host.local_player().unwrap();
            assert_eq!(player.entity().id(), 7);
            assert_eq!(pearl.projectile_owner, Some(player.entity()));
            let living = host.living(player.entity()).unwrap();
            let weapon = living.hand_item(host, Hand::Main).unwrap();
            assert_eq!(weapon.item().id(host), "minecraft:iron_sword");
            assert_eq!(living.is_blocking(host), Some(true));
            assert_eq!(
                player.within_entity_reach(host, pig_ref, EntityReach::Attack),
                Some(true)
            );
            assert_eq!(pig_ref.distance_to_sqr(host, CAMERA), Some(37.0));
            let level = host.level("minecraft:overworld").unwrap();
            assert_eq!(
                level.block(host, BLOCK).unwrap().id(host),
                "minecraft:stone"
            );

            if event.id == ENTITY_HITBOX_RENDERER && self.0.load(Ordering::Relaxed) != 0 {
                event.control.cancel_builtin();
                event
                    .draw
                    .lines(&[WorldLine::new(
                        [100.5, 64.0, 98.0],
                        [101.0, 64.0, 98.0],
                        0xFFFF0000,
                        1.0,
                        WorldDepth::Tested,
                    )])
                    .unwrap();
            }
        }
    }

    struct Later(Arc<AtomicUsize>);
    #[mod_api::__subscriptions]
    impl Hooks for Later {
        fn debug_renderer_view(&self, _: &dyn ClientHost, event: &mut DebugRendererViewEvent<'_>) {
            if event.id == ENTITY_HITBOX_RENDERER && event.control.builtin_cancelled() {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    if fresh_process("hitbox_replacement_keeps_other_gizmos_and_reads_the_client_view") {
        return;
    }
    with_client_game(|game| {
        let (pipe, _server) = horizon_protocol::connection::Pipe::pair();
        game.connect_local(pipe, "DebugHitboxes").unwrap();
        game.handle_packet(login()).unwrap();
        game.title = None;
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(100.0, 64.0, 100.0)).unwrap());
        game.selected = 0;
        game.inventory[0] = Some(ItemStack::new("minecraft:iron_sword", 1));
        game.offhand = Some(ItemStack::new("minecraft:shield", 1));
        game.using = Some((super::shield::OFFHAND_SLOT, 5));

        let mut chunk = Chunk::empty(6, 6);
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        chunk.set(BLOCK[0], BLOCK[1], BLOCK[2], stone.0);
        game.world.chunks.insert((6, 6), Arc::new(chunk));
        let registries = game.remote.as_ref().unwrap().builtin.clone();
        let kinds = registries.get("minecraft:entity_type").unwrap();
        for (id, kind, position, data) in [
            (PIG, "minecraft:pig", [104.0, 64.0, 99.0], 0),
            (PEARL, "minecraft:ender_pearl", [103.0, 65.0, 97.0], 7),
            (BOAT, "minecraft:oak_boat", [104.0, 63.5, 99.0], 0),
            (DRAGON, "minecraft:ender_dragon", [110.0, 80.0, 70.0], 0),
        ] {
            game.handle_packet(C::AddEntity(play::AddEntity {
                id: id as i32,
                uuid: id.into(),
                kind: kinds.id(kind).unwrap(),
                position,
                velocity: [0.0; 3],
                pitch: 0,
                yaw: 0,
                head_yaw: 0,
                data,
            }))
            .unwrap();
        }
        game.handle_packet(C::SetPassengers(play::SetPassengers {
            vehicle: BOAT as i32,
            passengers: vec![PIG as i32],
        }))
        .unwrap();
        // Two simulation samples, as read by real frame interpolation (not network target).
        let pig = game.mobs.iter_mut().find(|m| m.id == PIG).unwrap();
        pig.position.x = 106.0;

        game.debug_entries = DebugEntries::default();
        game.debug_entries
            .set_status("chunk_borders", Status::AlwaysOn);
        let mut draw = WorldGeometry::default();
        let render = |game: &mut super::Game<'_>, draw: &mut WorldGeometry| {
            game.fill_world_geometry(false, CAMERA, Mat4::IDENTITY, ALPHA, draw, &mut Vec::new())
        };
        let borders = render(game, &mut draw);
        assert!(!borders.lines.is_empty());
        game.debug_entries
            .set_status("entity_hitboxes", Status::AlwaysOn);
        let baseline = render(game, &mut draw);
        assert!(baseline.lines.starts_with(&borders.lines));
        assert!(baseline.lines.len() > borders.lines.len());

        let replacing = Arc::new(AtomicUsize::new(0));
        let later = Arc::new(AtomicUsize::new(0));
        mod_platform::__private::install(
            vec![
                ModEntry::new("replace", Box::new(Replace(replacing.clone()))),
                ModEntry::new("later", Box::new(Later(later.clone()))),
                ModEntry::new("marker", Box::new(Marker)),
            ]
            .into(),
        );
        push(&[SHOWN]);
        mods::register_debug_entries(ExecutionRole::Client);
        game.debug_entries.set_status(SHOWN, Status::AlwaysOn);
        let unchanged = render(game, &mut draw);
        assert_eq!(unchanged.lines, baseline.lines);
        assert_eq!(unchanged.on_top, baseline.on_top);
        assert_eq!(draw.batch().lines.len(), 1);

        replacing.store(1, Ordering::Relaxed);
        let cancelled = render(game, &mut draw);
        assert_eq!(later.load(Ordering::Relaxed), 1);
        assert!(game.debug_entry("entity_hitboxes"));
        assert_eq!(cancelled.lines, borders.lines);
        assert_eq!(cancelled.on_top, borders.on_top);
        let starts: Vec<_> = draw.batch().lines.iter().map(|line| line[0]).collect();
        assert_eq!(starts, [0.5, 0.25], "replacement and independent mod lines");

        game.debug_entries
            .set_status("entity_hitboxes", Status::Never);
        assert_eq!(render(game, &mut draw).lines, borders.lines);
        assert_eq!(later.load(Ordering::Relaxed), 1);
        assert_eq!(
            draw.batch().lines.len(),
            1,
            "the custom id still draws with F3+B off"
        );
    });
}
