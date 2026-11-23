use super::*;
/// The structure types registered at startup, for the executable to hand to
/// the engine's `StructureRegistry`.
pub(crate) fn installed_structure_type_list()
-> &'static [crate::world::structure_type::StructureTypeInfo] {
    installed().map_or(&[], |installed| installed.structure_types.as_slice())
}

pub(crate) fn installed_structure_candidate_list()
-> &'static [crate::world::structure_candidate::StructureCandidateInfo] {
    installed().map_or(&[], |installed| installed.structure_candidates.as_slice())
}

impl dyn Registrar + '_ {
    /// Register structure type `name` (`tower` or `mymod:tower`), the way
    /// NeoForge's registries `STRUCTURE_TYPE` and `STRUCTURE_PIECE` register
    /// once at startup: a data pack structure with `"type": "modid:tower"`
    /// then generates by `ty`'s code — its own start and pieces, placed by the
    /// game's own spacing, biome and terrain-adaptation checks. The type's id
    /// is in a namespace owned by the calling mod's manifest (`mymod:tower`); another mod's
    /// namespace, an empty name and an id registered twice refuse the mods at
    /// startup, naming the mod. `ty`'s `load_piece` is its saved pieces'
    /// loader: keep the type for as long as the game runs.
    pub fn register_structure_type<T: crate::StructureType>(&mut self, name: &str, ty: T) {
        self.add_structure_type(name, Arc::new(ty));
    }

    /// Register a native random-spread candidate selector, named by a level stem's
    /// `generator["horizon:structure_candidates"]` map from structure set id to selector id.
    ///
    /// A bare name uses this mod's namespace. Invalid or unowned ids and duplicate
    /// registrations refuse startup. Registration alone changes no world's placement.
    pub fn register_structure_candidate_selector<T: crate::StructureCandidateSelector>(
        &mut self,
        name: &str,
        selector: T,
    ) {
        self.add_structure_candidate_selector(name, Arc::new(selector));
    }
}

/// Registrar operations for structures.
pub trait StructuresRegistrar {
    /// Register structure type `name` with `ty`; mods reach it through
    /// `register_structure_type`.
    ///
    /// On WebAssembly, worker calls run under the linked group's store lock and call budget.
    fn add_structure_type(&mut self, name: &str, ty: Arc<dyn crate::StructureType>);

    /// Register a native selector; mods reach it through `register_structure_candidate_selector`.
    ///
    /// Not on wasm: the candidate selector is not bound by the generated worker interface.
    fn add_structure_candidate_selector(
        &mut self,
        name: &str,
        selector: Arc<dyn crate::StructureCandidateSelector>,
    ) {
        let _ = selector;
        panic!("structure candidate selector {name:?}: a wasm mod cannot register a selector");
    }
}
