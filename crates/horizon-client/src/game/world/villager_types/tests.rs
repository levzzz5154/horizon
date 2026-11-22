use super::*;
use horizon_assets::resource_source::{ResourceRoot, ResourceSource};
use horizon_client::mobs::{
    MobEvent, Species, render, spawn_rules::Reason, villager_types::type_id,
};
use mod_api::{
    Hooks, Identifier, ModEntry, RegisterVillagerTypesEvent, RegistrationContext,
    VillagerTypeDefinition,
};
use std::path::{Path, PathBuf};

const ID: &str = "outfit_check:meadow";
const VILLAGER: &str = "outfit_check:entity/villager/type/meadow";
const ZOMBIE: &str = "outfit_check:entity/zombie_villager/type/meadow";

fn definition() -> VillagerTypeDefinition {
    let id = |name| Identifier::parse(name).unwrap();
    VillagerTypeDefinition {
        key: id(ID),
        biomes: vec![id("minecraft:plains")],
        biome_tags: vec![],
        villager_texture: id(VILLAGER),
        zombie_villager_texture: id(ZOMBIE),
    }
}

struct OutfitCheck;

#[mod_api::__subscriptions]
impl Hooks for OutfitCheck {
    fn register_villager_types(
        &self,
        _: &RegistrationContext,
        event: &mut RegisterVillagerTypesEvent<'_>,
    ) {
        event.types.register(definition());
    }
}

#[test]
fn villager_type_selector_conflicts_and_tags() {
    let mut outfit = definition();
    outfit
        .biome_tags
        .push(Identifier::parse("outfit_check:mountains").unwrap());
    let declared = vec![("outfit_check".into(), outfit.clone())];
    let tags = HashMap::from([(
        "outfit_check:mountains".into(),
        HashSet::from(["minecraft:meadow".into(), "minecraft:plains".into()]),
    )]);
    let registry = Registry::prepare(&declared).unwrap();
    let outfits = bind(&declared, &registry, Some(&tags), |_| true).unwrap();
    assert_eq!(outfits.len(), 2);
    assert!(outfits.values().all(|kind| kind.id() == ID));
    assert!(
        bind(&declared, &registry, None, |_| true)
            .unwrap_err()
            .contains("unknown biome tag")
    );
    let mut conflicting = outfit.clone();
    conflicting.key = Identifier::parse("outfit_check:second").unwrap();
    let multiple = vec![
        ("outfit_check".into(), outfit.clone()),
        ("other".into(), conflicting),
    ];
    let registry = Registry::prepare(&multiple).unwrap();
    assert!(
        bind(&multiple, &registry, Some(&tags), |_| true)
            .unwrap_err()
            .contains("also selects")
    );
    assert!(
        Registry::prepare(&[declared[0].clone(), declared[0].clone()])
            .unwrap_err()
            .contains("duplicate")
    );
    outfit.key = Identifier::parse("minecraft:plains").unwrap();
    assert!(
        Registry::prepare(&[("outfit_check".into(), outfit)])
            .unwrap_err()
            .contains("reserved")
    );
}

fn outfit_resources(scratch: &Path) -> ResourceRoot {
    let base = PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
    let module = scratch.join("module");
    for (sprite, rgba) in [(VILLAGER, [255, 0, 180, 255]), (ZOMBIE, [0, 220, 255, 255])] {
        let (namespace, path) = sprite.split_once(':').unwrap();
        let path = module.join(format!("assets/{namespace}/textures/{path}.png"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut png = png::Encoder::new(std::fs::File::create(path).unwrap(), 64, 64);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header()
            .unwrap()
            .write_image_data(&rgba.repeat(64 * 64))
            .unwrap();
    }
    std::fs::write(module.join("horizon-mod.toml"), "id = 'outfit_check'\nname = 'Outfit fixture'\nversion = '1.0'\napi = '0.2'\nside = 'both'\ncapabilities = ['registries']\n").unwrap();
    let package = horizon_mod_package::Package::read(&module).unwrap();
    let mods = [horizon_client::mod_content::ContentMod {
        id: "outfit_check".into(),
        content: horizon_mod_package::Content::read(&module, &package.manifest).unwrap(),
        jars: vec![],
    }];
    let prepared = scratch.join("resources");
    horizon_client::mod_content::build(
        &base,
        Path::new(""),
        &mods,
        &prepared,
        &horizon_mod_package::enabled::EnabledMods::new([(
            "outfit_check".into(),
            mod_api::Side::Both,
        )]),
    )
    .unwrap();
    horizon_client::mod_content::resolved_resources(
        &base,
        prepared,
        Path::new(""),
        &mods,
        scratch.join("cache"),
    )
    .unwrap()
}

#[test]
#[ignore = "daily"]
fn villager_type_outfits_spawn_birth_save_and_reload() {
    use crate::game::{
        creatures::SpawnSite, native_entity_refs::tests::with_game_at, world_loading::WorldRequest,
    };
    let scratch = tempfile::tempdir().unwrap();
    let root = outfit_resources(scratch.path());
    let defaults = ResourceSource::directory(root.prepared().to_path_buf());
    let atlas = horizon_client::hud::HudAtlas::build_over(root.source(), &defaults).unwrap();
    for (texture, rgba) in [(VILLAGER, [255, 0, 180, 255]), (ZOMBIE, [0, 220, 255, 255])] {
        let rect = atlas.sprites[texture];
        let x = ((rect.u0 + rect.u1) * 0.5 * atlas.width as f32) as u32;
        let y = ((rect.v0 + rect.v1) * 0.5 * atlas.height as f32) as u32;
        let start = ((y * atlas.width + x) * 4) as usize;
        assert_eq!(atlas.pixels[start..start + 4], rgba);
    }
    drop(atlas);
    mod_platform::testing::with_mods(
        vec![ModEntry::new("outfit_check", Box::new(OutfitCheck))].into(),
        || {
            with_game_at(root.prepared().to_path_buf(), |game| {
                let mut request = WorldRequest::new(
                    "Outfit test",
                    Some("outfit"),
                    Some(1),
                    true,
                    None,
                    Arc::new(horizon_client::gamerules::RuleRegistry::vanilla()),
                )
                .unwrap();
                request.save_root = scratch.path().join("save");
                game.load_world(request).unwrap();
                game.install_client_villager_types().unwrap();
                let host: &dyn mod_api::Host = game;
                assert!(host.registry_entry("minecraft:villager_type", ID).is_some());
                assert_eq!(game.mob_maps().villager_type("minecraft:plains").id(), ID);
                assert_eq!(
                    game.mob_maps().villager_type("minecraft:desert").id(),
                    "minecraft:desert"
                );
                let set_biome = |game: &mut Game<'_>, id| {
                    let biome = world_biomes!(game).id(id);
                    let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
                    for qx in 0..4 {
                        for qy in 34..38 {
                            for qz in 0..4 {
                                chunk.set_biome_cell(qx, qy, qz, biome);
                            }
                        }
                    }
                    game.world.chunks.insert((0, 0), Arc::new(chunk));
                    assert_eq!(game.biome_name_at_block([7, 80, 8]).as_deref(), Some(id));
                };
                set_biome(game, "minecraft:plains");
                let spawn = |game: &mut Game<'_>, kind, x| {
                    game.spawn_mob(
                        kind,
                        [x, 80.0, 8.5],
                        true,
                        SpawnSite::Finalizing(Reason::Command),
                    )
                    .unwrap()
                };
                let parent = spawn(game, "minecraft:villager", 7.5);
                let partner = spawn(game, "minecraft:villager", 8.5);
                spawn(game, "minecraft:zombie_villager", 9.5);
                for inherited_from in [None, Some(partner)] {
                    if inherited_from.is_some() {
                        // When the parent's biome no longer selects this outfit,
                        // the parent's identity still does.
                        set_biome(game, "minecraft:desert");
                    }
                    game.handle_mob_events(vec![MobEvent::VillagerBirth {
                        parent,
                        partner,
                        child: Some(([8, 80, 8], inherited_from)),
                    }])
                    .unwrap();
                }
                let outfits = |game: &Game<'_>| {
                    let mut counts = [0; 3];
                    for mob in game.mobs.iter().filter(|mob| !mob.removed) {
                        let texture = match mob.species() {
                            Species::Villager => {
                                counts[usize::from(mob.is_baby())] += 1;
                                VILLAGER
                            }
                            Species::ZombieVillager => {
                                counts[2] += 1;
                                ZOMBIE
                            }
                            _ => continue,
                        };
                        assert_eq!(type_id(mob), ID);
                        assert!(
                            render::layers(mob, mob.age as f32, &Default::default())
                                .iter()
                                .any(|layer| layer.texture == texture)
                        );
                    }
                    assert_eq!(counts, [2, 2, 1]);
                };
                outfits(game);
                let names = game.entity_chunks(&HashSet::from([(0, 0)]), true);
                game.save.save_entities(names).unwrap();
                game.save.flush().unwrap();
                let tags = game.save.load_entities(0, 0);
                let data: Vec<_> = tags
                    .iter()
                    .filter_map(|tag| tag.get("VillagerData"))
                    .collect();
                assert!(
                    data.len() >= 4,
                    "the two villagers and two children must be saved"
                );
                for data in data {
                    assert_eq!(
                        horizon_client::save::text(data.get("type")).as_deref(),
                        Some(ID)
                    );
                }
                game.world.queue_entity_nbt((0, 0), tags);
                game.spawn_loaded_entities();
                outfits(game);
            });
        },
    );
}
