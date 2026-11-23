//! Native selectors for the candidate of an opted-in random-spread structure set.
use std::sync::Arc;

/// How vanilla draws the X and Z offsets in a random-spread cell.
// no wire: consumed only by a native structure candidate selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructureSpreadType {
    /// Draw one uniform offset per axis.
    Linear,
    /// Average two uniform offsets per axis.
    Triangular,
}

/// The one candidate of the placement cell, before the normal structure checks.
// no wire: returned only by a native structure candidate selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StructureCandidate {
    /// Use vanilla's random draw.
    Vanilla,
    /// Try this absolute chunk position instead. If it is outside this cell's
    /// allowed spread window, use vanilla's random draw instead.
    Chunk(crate::ChunkPos),
    /// Leave this cell without a candidate, including for exclusion checks.
    Skip,
}

/// The inputs to a structure candidate decision. No live world or registry handles are available.
// no wire: borrows the active generation state's identifiers in a native-only callback.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct StructureCell<'a> {
    /// The generating side: always the logical server, including in singleplayer.
    pub side: crate::LoopSide,
    /// The active level's namespaced id, not its dimension type.
    pub level: &'a str,
    /// The active level stem's generator type, such as `minecraft:noise` or `mymod:painted`.
    pub generator: &'a str,
    /// The id of the actual structure set from the world's resolved data, not a structure id.
    pub structure_set: &'a str,
    /// The world's seed.
    pub seed: i64,
    /// The signed cell indices [X, Z], using floor division of chunk coordinates by `spacing`.
    pub cell: [i32; 2],
    /// The width of a placement cell, in chunks.
    pub spacing: i32,
    /// The excluded margin at the high end of each cell axis, in chunks.
    pub separation: i32,
    /// How vanilla draws its offsets within the cell.
    pub spread: StructureSpreadType,
    /// The placement salt used to seed vanilla's draw.
    pub salt: i32,
}

impl<'a> StructureCell<'a> {
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        level: &'a str,
        generator: &'a str,
        structure_set: &'a str,
        seed: i64,
        cell: [i32; 2],
        spacing: i32,
        separation: i32,
        spread: StructureSpreadType,
        salt: i32,
    ) -> Self {
        Self {
            side: crate::LoopSide::Server,
            level,
            generator,
            structure_set,
            seed,
            cell,
            spacing,
            separation,
            spread,
            salt,
        }
    }
}

/// Native code that picks the candidate of a random-spread structure set's cell.
///
/// The game can ask about the same cell from several threads and more than once.
/// Return the same answer for the same inputs, independent of thread, call order and
/// mutable game state. Choosing a chunk does not bypass frequency, exclusions,
/// distance limits, biome checks, or validation of a structure's actual start.
pub trait StructureCandidateSelector: Send + Sync + 'static {
    /// Choose the candidate for `cell`, before the game's placement and start checks.
    fn candidate(&self, cell: &StructureCell<'_>) -> StructureCandidate;
}

pub(crate) struct StructureCandidateInfo {
    pub id: Box<str>,
    pub selector: Arc<dyn StructureCandidateSelector>,
}

/// Startup declarations, for the executable to hand to the world generator.
#[doc(hidden)]
pub fn structure_candidate_declarations() -> Vec<(String, Arc<dyn StructureCandidateSelector>)> {
    crate::parts::installed_structure_candidate_list()
        .iter()
        .map(|entry| (entry.id.to_string(), entry.selector.clone()))
        .collect()
}
