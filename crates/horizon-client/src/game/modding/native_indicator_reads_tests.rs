//! What an Enhanced Attack Indicator listener can read from the local client during play.
use super::{
    Game, Screen,
    native_client_session::tests::logged_client,
    native_entity_refs::tests::{run_fresh_cases, with_client_game},
};
use horizon_client::{chunk::Chunk, hud::HudBatch, items::ItemStack, motion::PlayerMotion};
use horizon_core::geometry::Vec3;
use horizon_protocol::{
    entity_data::{Value, serializer},
    packets::play::{self, Clientbound as C},
};
use mod_api::{
    BlockBreakingState, BlockPos, ClientHost, ClientSetupEvent, ComponentId, Flow,
    GuiLayerPreEvent, Hand, Hooks, ItemKey, ItemResource, ItemUse, ModEntry, Tag,
};
use std::sync::{Arc, Mutex};

const PARTIALS: [f32; 6] = [0.5, -10.0, 20.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
const OWN_ID: i32 = 7;
const HANDS: [Hand; 2] = [Hand::Main, Hand::Off];

#[derive(Debug, PartialEq)]
struct Held {
    item: ItemKey,
    count: u32,
    key: Option<String>,
    container: Option<Tag>,
    bundle: Option<Tag>,
    charged: Option<Tag>,
    food: Option<Tag>,
}

#[derive(Debug, PartialEq)]
struct Reads {
    charge: [Option<f32>; PARTIALS.len()],
    delay: Option<f32>,
    sleep: Option<u32>,
    mining: Option<BlockBreakingState>,
    cooldowns: [Option<f32>; 2],
    held: [Option<Held>; 2],
    use_state: Option<ItemUse>,
    using_item: Option<mod_api::ItemStack>,
}

#[derive(Default)]
struct Indicator(Mutex<Option<Reads>>);

impl Indicator {
    fn read(&self, host: &dyn ClientHost) {
        let held = HANDS.map(|hand| {
            let key = host.local_held_item_translation_key(hand);
            let Some(stack) = host.local_held_stack(hand) else {
                assert_eq!(key, None, "an empty hand has no translation key");
                return None;
            };
            let resource = ItemResource::of(&stack);
            let value = |id| resource.value(host, &ComponentId::new(id).unwrap());
            Some(Held {
                item: stack.item(),
                count: stack.count(),
                key,
                container: value("minecraft:container"),
                bundle: value("minecraft:bundle_contents"),
                charged: value("minecraft:charged_projectiles"),
                food: value("minecraft:food"),
            })
        });
        let reads = Reads {
            charge: PARTIALS.map(|partial| host.local_attack_strength_scale(partial)),
            delay: host.local_attack_strength_delay(),
            sleep: host.local_sleep_timer(),
            mining: host
                .local_player()
                .and_then(|player| player.block_breaking(host)),
            cooldowns: HANDS.map(|hand| host.local_held_cooldown_percent(hand)),
            held,
            use_state: host.local_item_use(),
            using_item: host.local_item_in_use(),
        };
        assert!(self.0.lock().unwrap().replace(reads).is_none());
    }

    fn take(&self) -> Reads {
        self.0.lock().unwrap().take().expect("indicator hook ran")
    }
}

struct Listener(Arc<Indicator>);

#[mod_api::__subscriptions]
impl Hooks for Listener {
    fn client_setup(&self, host: &mut dyn ClientHost, _: &mut ClientSetupEvent) {
        self.0.read(host);
    }

    fn gui_layer_pre(&self, host: &dyn ClientHost, event: &mut GuiLayerPreEvent<'_>) -> Flow {
        if event.layer == "minecraft:crosshair" {
            self.0.read(host);
        }
        Flow::Continue
    }
}

fn draw(game: &mut Game<'_>, indicator: &Indicator) -> Reads {
    let mut batch = HudBatch::new([320, 240], game.settings.gui_scale_inputs());
    assert!(
        game.gui_layer("minecraft:crosshair", &mut batch, |_, _| {})
            .is_some()
    );
    indicator.take()
}

fn tick(game: &mut Game<'_>) {
    game.tick_remote_level().unwrap();
}

fn assert_near(actual: Option<f32>, expected: f32) {
    let actual = actual.expect("live client read");
    assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
}

fn assert_charge(reads: &Reads, delay: f32, fractional: f32) {
    assert_near(reads.delay, delay);
    assert_near(reads.charge[0], fractional);
    assert_eq!(reads.charge[1..], [Some(0.0), Some(1.0), None, None, None]);
}

fn sync_attack_speed(game: &mut Game<'_>, modifiers: Vec<play::AttributeModifier>) {
    let attribute = game
        .motion
        .attributes
        .id("minecraft:attack_speed")
        .unwrap()
        .registry_id();
    game.handle_packet(C::UpdateAttributes(play::UpdateAttributes {
        id: OWN_ID,
        values: vec![play::AttributeSnapshot {
            attribute,
            base: 4.0,
            modifiers,
        }],
    }))
    .unwrap();
}

fn sleep_metadata(game: &mut Game<'_>, bed: Option<[i32; 3]>) {
    game.handle_packet(C::SetEntityData(play::SetEntityData {
        id: OWN_ID,
        values: vec![
            (
                6,
                serializer::POSE,
                Value::Id(if bed.is_some() { 2 } else { 0 }),
            ),
            (14, serializer::OPTIONAL_BLOCK_POS, Value::OptPos(bed)),
        ],
    }))
    .unwrap();
}

fn assert_contents(tag: &Option<Tag>, id: &str, count: i32) {
    let Some(Tag::List(contents)) = tag else {
        panic!("not a stack list: {tag:?}");
    };
    let [stack] = contents.as_slice() else {
        panic!("expected one stack: {contents:?}");
    };
    assert_eq!(stack.get("id"), Some(&Tag::String(id.into())));
    assert_eq!(stack.get("count"), Some(&Tag::Int(count)));
}

#[test]
#[ignore = "daily"]
fn enhanced_attack_indicator_reads_live_client_play() {
    if run_fresh_cases(
        "game::native_indicator_reads_tests::enhanced_attack_indicator_reads_live_client_play",
        "HORIZON_INDICATOR_READS_CASE",
        &["play"],
    ) {
        return;
    }
    let indicator = Arc::new(Indicator::default());
    let mods = vec![ModEntry::new(
        "indicator_reads",
        Box::new(Listener(indicator.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_client_game(|game| {
            game.native_client_setup();
            assert_eq!(
                indicator.take(),
                Reads {
                    charge: [None; PARTIALS.len()],
                    delay: None,
                    sleep: None,
                    mining: None,
                    cooldowns: [None; 2],
                    held: [None, None],
                    use_state: None,
                    using_item: None,
                }
            );

            let _server = logged_client(game);
            let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
            game.native_lifecycle
                .begin_open(mod_api::Authority::RemoteClient);
            game.title = None;
            game.screen = Screen::Playing;
            let mut player = PlayerMotion::new(game.motion, Vec3::new(4.5, 64.0, 4.5)).unwrap();
            player.entity_id = OWN_ID;
            player.ground.on_ground = true;
            game.player = Some(player);
            game.set_mode_id(0);
            game.yaw = 0.0;
            game.pitch = 0.0;
            game.streaming
                .insert_remote(&mut game.world, Chunk::empty(0, 0));
            game.poll_stationary_client().unwrap();
            let registry = game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let air = registry.default_state("minecraft:air").unwrap();
            for x in 3..=6 {
                for z in 3..=6 {
                    game.world.set(x, 63, z, stone).unwrap();
                }
            }

            // The client's real attack-speed attribute arrives from the server: the sword's
            // -2.4 modifier turns 4 attacks/s into 1.6, or 12.5 ticks for full strength.
            game.inventory[game.selected] = Some(ItemStack::new("minecraft:diamond_sword", 1));
            sync_attack_speed(
                game,
                vec![play::AttributeModifier {
                    id: "minecraft:base_attack_speed".into(),
                    amount: -2.4,
                    operation: 0,
                }],
            );
            tick(game); // A different main-hand item resets the charge.
            let sword = draw(game, &indicator);
            assert_charge(&sword, 12.5, 0.04);
            assert_eq!(
                sword.held[0].as_ref().unwrap().key.as_deref(),
                Some("item.minecraft.diamond_sword")
            );
            tick(game);
            tick(game);
            assert_charge(&draw(game, &indicator), 12.5, 0.2);

            game.handle_packet(C::SetHeldSlot(play::SetHeldSlot { slot: 1 }))
                .unwrap();
            sync_attack_speed(game, vec![]);
            tick(game);
            let empty = draw(game, &indicator);
            assert_charge(&empty, 5.0, 0.1);
            assert_eq!(empty.mining, Some(BlockBreakingState::Idle));
            assert_eq!(empty.sleep, Some(0));
            assert_eq!(empty.held, [None, None]);
            assert_eq!(empty.cooldowns, [Some(0.0); 2]);

            // Aim straight at stone and hold the actual attack mapping. A grounded bare hand
            // adds 1 / 1.5 / 100 per continuation tick because stone needs a tool for drops.
            game.world.set(4, 65, 6, stone).unwrap();
            game.set_mapping_down("key.attack", true).unwrap();
            game.click_mapping("key.attack").unwrap();
            for continuations in 1..=2 {
                tick(game);
                let breaking = draw(game, &indicator).mining;
                let Some(BlockBreakingState::Breaking {
                    position,
                    progress,
                    stage,
                }) = breaking
                else {
                    panic!("the held attack input should mine stone: {breaking:?}");
                };
                assert_eq!(position, BlockPos::new(4, 65, 6));
                assert_eq!(stage, Some(0));
                assert_near(Some(progress), continuations as f32 / 150.0);
            }
            game.set_mapping_down("key.attack", false).unwrap();
            tick(game);
            assert_eq!(
                draw(game, &indicator).mining,
                Some(BlockBreakingState::Idle)
            );
            game.world.set(4, 65, 6, air).unwrap();

            // The two real cooldown packets apply different durations. Main's item ID is not
            // its cooldown group; its stack overrides that group with indicator:main.
            let mut bundle = ItemStack::new("minecraft:bundle", 1);
            let data = bundle.data_mut();
            data.bundle_contents
                .push(ItemStack::new("minecraft:apple", 3));
            data.set_other(
                "minecraft:use_cooldown",
                Some(Tag::compound([
                    ("seconds", Tag::Float(1.0f32.to_bits())),
                    ("cooldown_group", Tag::String("indicator:main".into())),
                ])),
            );
            game.inventory[game.selected] = Some(bundle);
            game.offhand = Some(ItemStack::new("minecraft:apple", 2));
            for (group, duration) in [("indicator:main", 20), ("minecraft:apple", 10)] {
                game.handle_packet(C::Cooldown(play::Cooldown {
                    group: group.into(),
                    duration,
                }))
                .unwrap();
            }
            for _ in 0..5 {
                tick(game);
            }
            let cooldown = draw(game, &indicator);
            assert_eq!(cooldown.cooldowns, [Some(0.75), Some(0.5)]);
            let [Some(bundle), Some(apple)] = cooldown.held else {
                panic!("both hands populated");
            };
            assert_contents(&bundle.bundle, "minecraft:apple", 3);
            assert_eq!(apple.count, 2);
            assert_eq!(
                apple.food.as_ref().and_then(|food| food.get("nutrition")),
                Some(&Tag::Int(4))
            );
            game.offhand
                .as_mut()
                .unwrap()
                .data_mut()
                .set_other("!minecraft:food", Some(Tag::Compound(Default::default())));
            assert_eq!(draw(game, &indicator).held[1].as_ref().unwrap().food, None);

            let mut box_stack = ItemStack::new("minecraft:white_shulker_box", 1);
            box_stack
                .data_mut()
                .container
                .push((5, ItemStack::new("minecraft:apple", 7)));
            let mut crossbow = ItemStack::new("minecraft:crossbow", 1);
            crossbow
                .data_mut()
                .charged
                .push(ItemStack::new("minecraft:arrow", 1));
            game.inventory[game.selected] = Some(box_stack);
            game.offhand = Some(crossbow);
            let [Some(container), Some(crossbow)] = draw(game, &indicator).held else {
                panic!("both hands populated");
            };
            assert_eq!(
                container.key.as_deref(),
                Some("block.minecraft.white_shulker_box")
            );
            let Some(Tag::List(slots)) = container.container else {
                panic!("container contents");
            };
            let [slot] = slots.as_slice() else {
                panic!("one occupied slot: {slots:?}");
            };
            assert_eq!(slot.get("slot"), Some(&Tag::Int(5)));
            assert_eq!(
                slot.get("item").and_then(|stack| stack.get("count")),
                Some(&Tag::Int(7))
            );
            let Some(Tag::List(projectiles)) = crossbow.charged else {
                panic!("charged projectile list");
            };
            let [projectile] = projectiles.as_slice() else {
                panic!("one charged projectile: {projectiles:?}");
            };
            assert_eq!(
                projectile.get("id"),
                Some(&Tag::String("minecraft:arrow".into()))
            );

            game.inventory[game.selected] = Some(ItemStack::new("minecraft:stick", 1));
            game.inventory[9] = Some(ItemStack::new("minecraft:arrow", 8));
            let mut bow = ItemStack::new("minecraft:bow", 1);
            let custom_name = Tag::compound([("text", Tag::String("Ranged test".into()))]);
            bow.data_mut().custom_name = Some(custom_name.clone());
            game.offhand = Some(bow);
            game.click_mapping("key.use").unwrap();
            game.set_mapping_down("key.use", true).unwrap();
            tick(game);
            let drawing = draw(game, &indicator);
            assert_eq!(
                drawing.use_state.map(|use_state| use_state.hand),
                Some(Hand::Off)
            );
            let [Some(stick), Some(bow)] = drawing.held else {
                panic!("both hands populated");
            };
            assert_eq!(stick.key.as_deref(), Some("item.minecraft.stick"));
            assert_eq!(
                bow.key.as_deref(),
                Some("item.minecraft.bow"),
                "not the custom name"
            );
            let used = drawing.using_item.expect("an offhand bow in use");
            assert_eq!(
                used.item(),
                bow.item,
                "the use belongs to the offhand bow, not the stick"
            );
            let patch = used.components().unwrap().as_nbt();
            assert_eq!(
                patch.get(&"minecraft:custom_name".into()),
                Some(&custom_name)
            );
            game.set_mapping_down("key.use", false).unwrap();
            tick(game);
            let released = draw(game, &indicator);
            assert_eq!(released.use_state, None);
            assert_eq!(released.using_item, None);

            // Sleep starts from the server's entity metadata. The wake animation triggers the
            // local overlay fade, whose timer still increases although the player is awake.
            sleep_metadata(game, Some([5, 64, 5]));
            assert_eq!(draw(game, &indicator).sleep, Some(0));
            for expected in 1..=3 {
                tick(game);
                assert_eq!(draw(game, &indicator).sleep, Some(expected));
            }
            sleep_metadata(game, None);
            game.handle_packet(C::Animate(play::Animate {
                id: OWN_ID,
                action: 0,
            }))
            .unwrap();
            assert_eq!(draw(game, &indicator).sleep, Some(100));
            for expected in 101..=109 {
                tick(game);
                assert_eq!(draw(game, &indicator).sleep, Some(expected));
            }
            tick(game);
            assert_eq!(draw(game, &indicator).sleep, Some(0));
        });
    });
}
