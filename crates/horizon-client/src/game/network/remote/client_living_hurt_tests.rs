use crate::game::{native_entity_refs::tests::with_client_game, native_lifecycle::NativeLifecycle};
use horizon_client::{level_loading::Reason, motion::PlayerMotion};
use horizon_core::geometry::Vec3;
use horizon_protocol::{
    connection::Pipe,
    packets::play::{self, Clientbound as C},
};
use mod_api::{ClientHost, ClientLivingHurtEvent, Hooks};
use std::sync::{Arc, Mutex};

type Notice = (u32, Option<u32>, Option<u32>);
struct Probe(Arc<Mutex<Vec<Notice>>>);

#[mod_api::__subscriptions]
impl Hooks for Probe {
    fn client_living_hurt(&self, host: &mut dyn ClientHost, event: &mut ClientLivingHurtEvent) {
        assert_eq!(host.entity(event.entity.id()), Some(event.entity));
        assert_eq!(event.entity.position(host), Some([10.0, 64.0, 12.0]));
        assert_eq!(
            event.entity.kind(host).map(|key| key.id(host)).as_deref(),
            Some("minecraft:creeper")
        );
        self.0.lock().unwrap().push((
            event.entity.id(),
            event.causing_entity.map(|entity| entity.id()),
            event.direct_entity.map(|entity| entity.id()),
        ));
    }
}

#[test]
#[ignore = "daily"]
fn client_living_hurt_keeps_same_tick_notices_and_ignores_tilt_and_stale_victims() {
    // Keep observations local to this fixture and exercise the real generated subscription.
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "client_living_hurt_probe",
        Box::new(Probe(calls.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_client_game(|game| {
            game.native_lifecycle = NativeLifecycle::default();
            let (client, _server) = Pipe::pair();
            game.connect_local(client, "ClientHurtProbe").unwrap();
            game.enter_remote_dimension(
                &play::SpawnInfo {
                    dimension_type: 0,
                    dimension: "minecraft:overworld".into(),
                    seed: 0,
                    game_type: 0,
                    previous_game_type: None,
                    debug: false,
                    flat: true,
                    last_death: None,
                    portal_cooldown: 0,
                    sea_level: 63,
                },
                false,
                Reason::Other,
            );
            let remote = game.remote.as_mut().unwrap();
            remote.logged_in = true;
            remote.entity_id = 7;
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());

            for (id, kind) in [(31, "minecraft:creeper"), (32, "minecraft:arrow")] {
                let kind = game
                    .remote
                    .as_ref()
                    .unwrap()
                    .builtin
                    .get("minecraft:entity_type")
                    .unwrap()
                    .id(kind)
                    .unwrap();
                game.handle_packet(C::AddEntity(play::AddEntity {
                    id,
                    uuid: id as u128,
                    kind,
                    position: [10.0, 64.0, 12.0],
                    velocity: [0.0; 3],
                    pitch: 0,
                    yaw: 0,
                    head_yaw: 0,
                    data: 0,
                }))
                .unwrap();
            }

            let hit = |id, cause, direct| {
                C::DamageEvent(play::DamageEvent {
                    id,
                    source_type: 0,
                    cause,
                    direct,
                    position: None,
                })
            };
            game.handle_packet(hit(31, Some(7), Some(32))).unwrap();
            assert_eq!(game.mob_index.get(&game.mobs, 31).unwrap().hurt_time, 10);
            game.handle_packet(C::HurtAnimation(play::HurtAnimation { id: 31, yaw: 0.0 }))
                .unwrap();
            game.handle_packet(hit(31, None, Some(99))).unwrap();
            assert_eq!(game.mob_index.get(&game.mobs, 31).unwrap().hurt_time, 10);
            game.handle_packet(hit(32, Some(7), None)).unwrap();
            game.handle_packet(C::RemoveEntities(play::RemoveEntities { ids: vec![31] }))
                .unwrap();
            game.handle_packet(hit(31, Some(7), Some(32))).unwrap();
        });
    });
    assert_eq!(
        calls.lock().unwrap().as_slice(),
        [(31, Some(7), Some(32)), (31, None, None)]
    );
}
