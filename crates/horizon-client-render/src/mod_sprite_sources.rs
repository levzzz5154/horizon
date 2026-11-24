//! The atlas sprite source types native mods register (NeoForge's `RegisterSpriteSourcesEvent`),
//! kept once at start, and the hook point that asks a mod's source for its sprites.
use horizon_assets::resource_source::{ResourceSource, stays_inside};
use horizon_core::nbt::Tag;
use mod_api::{HookId, Identifier, RegistrationContext, ResourceReader, SpriteSourceOutput};
use std::{collections::BTreeSet, sync::RwLock};

static TYPES: RwLock<BTreeSet<String>> = RwLock::new(BTreeSet::new());

/// Keeps the sprite source types native mods registered, before resources load. An id that is not
/// `<namespace>:<path>` in a namespace other than vanilla's, or one registered already, is logged
/// and left out. Returns the ids kept, in registration order.
pub fn register(ids: Vec<String>) -> Vec<String> {
    horizon_assets::item_model::keep_mod_ids(&TYPES, "sprite source type", ids)
}

/// Whether a native mod registered `id` as a sprite source type.
pub fn is_registered(id: &str) -> bool {
    TYPES.read().unwrap_or_else(|e| e.into_inner()).contains(id)
}

/// Whether any native mod has registered at least one sprite source type.
pub fn has_registered() -> bool {
    !TYPES.read().unwrap_or_else(|e| e.into_inner()).is_empty()
}

/// Scoped [`ResourceReader`] backed by an active client [`ResourceSource`].
pub struct SourceResourceReader<'a> {
    source: &'a ResourceSource,
}

impl<'a> SourceResourceReader<'a> {
    /// Wrap `source` for the duration of a sprite-source callback.
    pub fn new(source: &'a ResourceSource) -> Self {
        Self { source }
    }
}

fn asset_path(id: &Identifier) -> Option<String> {
    let path = format!("assets/{}/{}", id.namespace(), id.path());
    stays_inside(&path).then_some(path)
}

impl ResourceReader for SourceResourceReader<'_> {
    fn read(&self, id: &Identifier) -> Option<Vec<u8>> {
        let path = asset_path(id)?;
        self.source.read(&path).ok()
    }

    fn read_stack(&self, id: &Identifier) -> Vec<Vec<u8>> {
        let Some(path) = asset_path(id) else {
            return Vec::new();
        };
        self.source.read_stack(&path)
    }

    fn list(&self, prefix: &str, suffix: &str) -> Vec<Identifier> {
        let mut out = Vec::new();
        for file in self.source.list("assets/") {
            let Some((namespace, path)) =
                file.strip_prefix("assets/").and_then(|r| r.split_once('/'))
            else {
                continue;
            };
            if path.starts_with(prefix)
                && path.ends_with(suffix)
                && let Ok(id) = Identifier::parse(&format!("{namespace}:{path}"))
            {
                out.push(id);
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// Hook point for native mods (docs/modding/src/how-to/native-mods.md, declared in `hooks/` with
/// the game's): an atlas definition's entry of a type a mod registered (`SpriteSource.run`).
/// `config` is the entry's object, `listed` the sprites the entries before it listed; the mod that
/// owns `source` pushes its sprites to `output`.
#[inline(never)]
pub fn native_sprite_source(
    context: &RegistrationContext,
    source: &str,
    atlas: &Identifier,
    config: &Tag,
    listed: &[Identifier],
    output: &mut Vec<SpriteSourceOutput>,
) {
    if !mod_api::__private::listened(HookId::SpriteSource) {
        return;
    }
    mod_api::dispatch_sprite_source(context, source, atlas, config, listed, output);
}

/// Hook point for native mods: an atlas definition's entry of a type a mod registered
/// (`SpriteSource.run`), handed a scoped [`ResourceReader`] over the active client resource packs.
#[inline(never)]
pub fn native_sprite_source_read(
    context: &RegistrationContext,
    source: &str,
    atlas: &Identifier,
    config: &Tag,
    resources: &dyn ResourceReader,
    output: &mut Vec<SpriteSourceOutput>,
) {
    if !mod_api::__private::listened(HookId::SpriteSourceRead) {
        return;
    }
    mod_api::dispatch_sprite_source_read(context, source, atlas, config, resources, output);
}

/// What the mods' sources answer for the entries of `atlas`: the `ask` of
/// [`crate::atlas_sources::list`]. It runs on the resource loader, as a registration.
pub fn ask(
    atlas: &Identifier,
) -> impl FnMut(&str, &Tag, &[Identifier]) -> Vec<SpriteSourceOutput> + '_ {
    use mod_api::__private::game::RegistrationContextNew;
    move |source, config, listed| {
        let _phase =
            mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::Registration);
        let mut output = Vec::new();
        native_sprite_source(
            &RegistrationContext::new(),
            source,
            atlas,
            config,
            listed,
            &mut output,
        );
        output
    }
}

/// What the mods' sources answer for the entries of `atlas` with a scoped [`ResourceReader`]:
/// dispatches both `sprite_source` (for existing listeners) and `sprite_source_read` (with
/// `resources`).
pub fn ask_with_resources<'a>(
    atlas: &'a Identifier,
    resources: &'a dyn ResourceReader,
) -> impl FnMut(&str, &Tag, &[Identifier]) -> Vec<SpriteSourceOutput> + 'a {
    use mod_api::__private::game::RegistrationContextNew;
    move |source, config, listed| {
        let _phase =
            mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::Registration);
        let context = RegistrationContext::new();
        let mut output = Vec::new();
        native_sprite_source(&context, source, atlas, config, listed, &mut output);
        native_sprite_source_read(&context, source, atlas, config, resources, &mut output);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atlas_sources::{SpriteOrigin, SpriteSource, decode, list};

    fn id(text: &str) -> Identifier {
        Identifier::parse(text).unwrap()
    }
    fn textures() -> Vec<String> {
        [
            "srctest:item/apple",
            "srctest:item/gem",
            "srctest:item/stick",
        ]
        .map(String::from)
        .to_vec()
    }
    fn pixels(sprite: &str, width: u32, height: u32, rgba: Vec<u8>) -> SpriteSourceOutput {
        SpriteSourceOutput::Pixels {
            sprite: id(sprite),
            width,
            height,
            rgba,
        }
    }
    /// A definition of a directory source, the registered source, then a single file.
    fn definition() -> Vec<SpriteSource> {
        register(vec!["srctest:own".into()]);
        decode(
            r#"{"sources":[
                {"type":"directory","source":"item","prefix":"item/"},
                {"type":"srctest:own","shade":7},
                {"type":"single","resource":"srctest:item/stick","sprite":"srctest:late"}
            ]}"#,
        )
        .unwrap()
    }

    #[test]
    fn registration_keeps_only_new_ids_in_a_mods_namespace() {
        assert_eq!(
            register(vec![
                "srcreg:one".into(),
                "srcreg:one".into(),
                "minecraft:mine".into(),
                "no_namespace".into(),
                "srcreg:two".into(),
            ]),
            ["srcreg:one", "srcreg:two"]
        );
        assert!(is_registered("srcreg:one") && is_registered("srcreg:two"));
        assert!(!is_registered("minecraft:mine") && !is_registered("no_namespace"));
        assert!(
            register(vec!["srcreg:one".into()]).is_empty(),
            "already registered"
        );
    }

    #[test]
    fn an_unregistered_type_fails_its_definition_by_name() {
        let text = r#"{"sources":[{"type":"directory","source":"item","prefix":""},{"type":"srcfree:nobody"}]}"#;
        assert_eq!(
            decode(text).unwrap_err().to_string(),
            "sources[1]: unknown sprite source type srcfree:nobody"
        );
        register(vec!["srcfree:nobody".into()]);
        assert!(matches!(
            decode(text).unwrap().as_slice(),
            [_, SpriteSource::Registered { kind, .. }] if kind == "srcfree:nobody"
        ));
    }

    #[test]
    fn the_source_is_asked_with_its_entry_and_what_was_listed_before_it() {
        let mut asked = Vec::new();
        list(&definition(), &textures(), &mut |kind, config, listed| {
            assert!(
                matches!(config, Tag::Compound(_)),
                "an object converts to a compound"
            );
            asked.push((
                kind.to_owned(),
                config.get("shade").cloned(),
                listed.to_vec(),
            ));
            Vec::new()
        });
        assert_eq!(
            asked,
            [(
                "srctest:own".to_owned(),
                // A JSON number converts to the narrowest tag that holds it.
                Some(Tag::Byte(7)),
                vec![
                    id("srctest:item/apple"),
                    id("srctest:item/gem"),
                    id("srctest:item/stick")
                ]
            )]
        );
    }

    #[test]
    fn outputs_apply_in_order_and_a_later_sprite_replaces_an_earlier_one() {
        let listed = list(&definition(), &textures(), &mut |_, _, _| {
            vec![
                // Replaces the directory's apple, then is replaced by the next output.
                pixels("srctest:item/apple", 1, 1, vec![1, 1, 1, 255]),
                SpriteSourceOutput::Texture {
                    sprite: id("srctest:item/apple"),
                    texture: id("srctest:item/gem"),
                },
                pixels("srctest:new", 1, 2, vec![9; 8]),
                // The single source after the mod's replaces this one.
                pixels("srctest:late", 1, 1, vec![0; 4]),
            ]
        });
        assert_eq!(
            listed["srctest:item/apple"],
            SpriteOrigin::File("srctest:item/gem".into())
        );
        assert_eq!(
            listed["srctest:new"],
            SpriteOrigin::Pixels((1, 2, vec![9; 8]))
        );
        assert_eq!(
            listed["srctest:late"],
            SpriteOrigin::File("srctest:item/stick".into())
        );
        let generated = crate::atlas_sources::generate(&listed["srctest:new"], &|_, _| None);
        assert_eq!(
            generated,
            Some((1, 2, vec![9; 8])),
            "a mod's pixels need no file"
        );
    }

    #[test]
    fn remove_drops_a_listed_sprite_and_an_unlisted_id_changes_nothing() {
        let listed = list(&definition(), &textures(), &mut |_, _, _| {
            vec![
                SpriteSourceOutput::Remove {
                    sprite: id("srctest:item/gem"),
                },
                SpriteSourceOutput::Remove {
                    sprite: id("srctest:never_listed"),
                },
            ]
        });
        let ids: Vec<&str> = listed.keys().map(String::as_str).collect();
        assert_eq!(
            ids,
            ["srctest:item/apple", "srctest:item/stick", "srctest:late"]
        );
    }

    #[test]
    fn what_cant_be_a_sprite_is_skipped_alone() {
        let side = 32768_u32;
        let listed = list(&definition(), &textures(), &mut |_, _, _| {
            vec![
                pixels("srctest:short", 2, 2, vec![0; 15]),
                pixels("srctest:long", 2, 2, vec![0; 17]),
                pixels("srctest:empty", 0, 4, Vec::new()),
                // The product overflows: refused without a panic.
                pixels("srctest:overflow", u32::MAX, u32::MAX, vec![0; 4]),
                pixels(
                    "srctest:too_wide",
                    side + 1,
                    1,
                    vec![0; (side as usize + 1) * 4],
                ),
                SpriteSourceOutput::Texture {
                    sprite: id("srctest:missing"),
                    texture: id("srctest:item/nothing"),
                },
                pixels("srctest:widest", side, 1, vec![0; side as usize * 4]),
                pixels("srctest:good", 2, 2, vec![0; 16]),
            ]
        });
        let ids: Vec<&str> = listed.keys().map(String::as_str).collect();
        assert_eq!(
            ids,
            [
                "srctest:good",
                "srctest:item/apple",
                "srctest:item/gem",
                "srctest:item/stick",
                "srctest:late",
                "srctest:widest"
            ]
        );
    }

    #[test]
    fn source_resource_reader_reads_stacks_and_lists_assets() {
        use horizon_assets::resource_source::Resource;
        use std::collections::BTreeMap;

        let base = ResourceSource::indexed(BTreeMap::from([
            (
                "assets/demo/textures/block/base.png".into(),
                Resource::bytes(b"base_only".to_vec()),
            ),
            (
                "assets/demo/textures/block/shared.png".into(),
                Resource::bytes(b"lower".to_vec()),
            ),
            (
                "assets/demo/textures/item/sword.png".into(),
                Resource::bytes(b"sword".to_vec()),
            ),
            (
                "data/demo/ignored.json".into(),
                Resource::bytes(b"{}".to_vec()),
            ),
        ]));
        let layered = ResourceSource::layered(
            BTreeMap::from([
                (
                    "assets/demo/textures/block/base.png".into(),
                    Resource::file(base.clone(), "assets/demo/textures/block/base.png".into()),
                ),
                (
                    "assets/demo/textures/block/shared.png".into(),
                    Resource::bytes(b"upper".to_vec()),
                ),
                (
                    "assets/demo/textures/block/added.png".into(),
                    Resource::bytes(b"top_only".to_vec()),
                ),
            ]),
            base,
        );
        let reader = SourceResourceReader::new(&layered);
        assert_eq!(
            reader.read(&id("demo:textures/block/shared.png")),
            Some(b"upper".to_vec())
        );
        assert_eq!(reader.read(&id("demo:textures/block/missing.png")), None);
        assert_eq!(
            reader.read_stack(&id("demo:textures/block/shared.png")),
            vec![b"lower".to_vec(), b"upper".to_vec()],
            "lowest priority first, highest priority last"
        );
        assert_eq!(
            reader.read_stack(&id("demo:textures/block/base.png")),
            vec![b"base_only".to_vec()],
            "pass-through underlay entries are not duplicated"
        );
        assert_eq!(
            reader.read_stack(&id("demo:textures/block/added.png")),
            vec![b"top_only".to_vec()]
        );
        assert_eq!(
            reader.list("textures/block/", ".png"),
            vec![
                id("demo:textures/block/added.png"),
                id("demo:textures/block/base.png"),
                id("demo:textures/block/shared.png"),
            ]
        );
    }
}
