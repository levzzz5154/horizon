# Villager professions and trades

Declare custom professions once per mod load through
`Hooks::register_villager_professions`. The callback receives a
`RegistrationContext` and a `RegisterVillagerProfessionsEvent`; call
`event.professions.register(definition)` on its `VillagerProfessions` registrar.
The engine records the declaring mod as owner. World mutation is unavailable
at registration, and datapack reload resolves the declarations again without
rerunning the callback. Native and wasm mods use the same declaration API.

## Definitions and resources

`ProfessionDefinition` contains:

- A `ProfessionKey`, wrapping a qualified `Identifier` such as `example:engineer`.
- `workstations`: `ProfessionWorkstation` selectors, each with a `PoiKey`, block
  identifier and required block-state properties. Omitted properties match all
  values.
- `held_poi` and `acquirable_poi`: `PoiKey` lists for retaining and acquiring a job.
- `requested_items`, `secondary_blocks` and an optional `work_sound`, using
  resource identifiers. Requested items do not grant farmer AI.
- `trades`: explicit level-to-`TradeSetKey` pairs for levels 1–5. A missing level
  adds no offers; an empty map is legal.

`PoiKey` and `TradeSetKey` also wrap `Identifier`. A trade-set key is independent
of the profession key: no namespace or level suffix is inferred. Provide trade
sets and offers through the `trade_set` and `villager_trade` datapack
registries; trade JSON alone does not register a profession.

## Registration refusals

References resolve against the complete candidate tables before publication.
`ProfessionRegistrationError` describes refusals:

| Variant | Reason |
|---|---|
| `DuplicateProfession` | Multiple declarations use one profession key; owners are reported in sorted order. |
| `ReservedProfession` | A profession key uses the reserved `minecraft:` namespace. |
| `OverlappingPoi` | Two professions acquire jobs from the same custom POI. |
| `UnacquirablePoi` | A declared custom workstation has no acquiring profession. |
| `InvalidWorkstation` | A workstation selector is invalid or conflicts with another POI. |
| `UnknownPoi`, `UnknownTradeSet`, `UnknownItem` | A referenced POI, trade set or item is missing. |
| `UnknownBlock`, `UnknownSound` | A secondary block or work sound is missing. |
| `InvalidTradeLevel` | A level is outside 1–5 or appears more than once. |

Custom workstation POI identities must also stay outside `minecraft:`. Missing
workstation blocks and selectors matching no valid states refuse preparation.
A failed candidate publishes nothing; a failed reload keeps the active tables.

## Identity, ordering and multiplayer

Profession and custom POI table order depends only on registered qualified IDs,
not mod load order or declaring owner. Vanilla's 15 professions retain the first
15 profession slots; custom professions follow in sorted order. Numeric slots
are runtime lookups, not save identities. Saves retain qualified profession and
POI names. A missing profession remains unresolved without discarding its name
or existing offers, so restoring the mod can resolve it again.

Clients build the same deterministic identity table from their installed mods and
use it for villager rendering. Install the profession mod on both the server and
its clients, with the mod declared for both sides. The mod-list handshake refuses
missing or mismatched both-side mods; client-only mods do not take part in that
comparison. Missing professions are not replaced with vanilla
identities.

## Add a biome-selected villager type

A type selects a villager's clothing, independently of its profession and
trades. Implement `Hooks::register_villager_types` to declare the persistent ID,
biome selectors, and separate villager and zombie-villager clothing textures.
The hook runs once per mod load and does not allow world mutation.

Create a project with `cargo horizon-mod new meadow_folk --template native`
using an SDK that includes the hook. Set `side = "both"` in `horizon-mod.toml`
and replace the template's `src/lib.rs` with:

```rust,ignore
use mod_api::{
    Hooks, Identifier, RegisterVillagerTypesEvent, RegistrationContext, VillagerTypeDefinition,
};

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_villager_types(
        &self,
        _: &RegistrationContext,
        event: &mut RegisterVillagerTypesEvent<'_>,
    ) {
        let id = |value: &str| Identifier::parse(value).unwrap();
        event.types.register(VillagerTypeDefinition {
            key: id("meadow_folk:meadow"),
            biomes: vec![id("minecraft:meadow")],
            biome_tags: vec![],
            villager_texture: id("meadow_folk:entity/villager/type/meadow"),
            zombie_villager_texture: id("meadow_folk:entity/zombie_villager/type/meadow"),
        });
    }
}
```

Use qualified IDs in `biomes` or `biome_tags`; tag IDs have no leading `#`.
The type key must be unique and outside `minecraft:`. A biome cannot select
two different custom types, even if the overlap comes from biome tags.

Texture identifiers are relative to `textures/` and omit the `.png` extension.
For this example, put the clothing PNGs at:

- `assets/meadow_folk/textures/entity/villager/type/meadow.png`
- `assets/meadow_folk/textures/entity/zombie_villager/type/meadow.png`

From the mod directory, run
`horizon-launcher native-build . --instance "My Instance" --trust-native-code`
against the matching Horizon build, then start that instance. In a world with
commands enabled, use `/locate biome minecraft:meadow`, go to the meadow and
spawn new villagers and zombie villagers with eggs or `/summon` without NBT.

For a Wasm project instead, use `--template wasm`, package it with
`cargo horizon-mod build --wasm`, and install the resulting `.hmod` on both sides.

Start with strongly different clothing colors for the two species. Give two
meadow villagers food and access to a spare bed: their child takes the biome
or a parent's type. Save and quit, then reopen the world to check that these
villagers still wear their outfits. Fresh spawns in unselected biomes keep
their vanilla outfits.

For multiplayer, install the same mod on the server and Horizon clients.
Accepted vanilla clients see the plains outfit for custom types; this does
not change the saved type. This hook does not register hat visuals.
