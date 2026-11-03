//! Removing an entity for native server mods, through an `EntityRef`
//! (docs/native-mods.md, "Entity and level handles", #7387): `Entity.discard`
//! and `Entity.remove` with a reason. Nothing dies and nothing drops loot.
//! The handle reads as gone from the call on, as `isRemoved` does. A mob
//! leaves at once; the other kinds leave at the level's next sweep of
//! departed entities, a few times per tick, so a hook that removes one never
//! shifts the lists the game is walking. A vehicle with a chest spills it, as vanilla's `remove` does.
use super::{Game, commands::Target};
use horizon_client::mod_api::{EntityRef, NativeRemovalReason};
use horizon_core::position::BlockPos;

impl Game<'_> {
    /// Removes the entity with no death and no loot (`Entity.discard`): a
    /// chest boat or a minecart with a chest still drops what it holds, and
    /// a boat its lead, as vanilla's `remove` does. False for a player, or a
    /// stale handle. API @API:entity_remove@.
    pub(super) fn native_entity_discard(&mut self, entity: EntityRef) -> bool {
        self.native_entity_remove(entity, NativeRemovalReason::Discarded)
    }
    /// Removes the entity for `reason` (`Entity.remove`): `Discarded`, as
    /// `discard`, or `Killed`, which also runs a mob's effects that act on
    /// removal (Oozing's slimes, Weaving's cobwebs, Wind Charged's burst),
    /// still with no death and no loot. False for a player, or a stale
    /// handle. API @API:entity_remove@.
    pub(super) fn native_entity_remove(
        &mut self,
        entity: EntityRef,
        reason: NativeRemovalReason,
    ) -> bool {
        let killed = reason == NativeRemovalReason::Killed;
        match self.native_entity_target(entity) {
            None | Some(Target::Player | Target::Guest(_)) => false,
            Some(Target::Mob(id)) => {
                self.remove_mob_now(id, killed);
                true
            }
            Some(t) => {
                if !self.persist.removals.contains(&t) {
                    self.persist.removals.push(t);
                }
                true
            }
        }
    }
    /// A mob leaves the level: its effects act first when it was killed.
    fn remove_mob_now(&mut self, id: u32, killed: bool) {
        let Some(i) = self.mobs.iter().position(|m| m.id == id && !m.removed) else {
            return;
        };
        if killed && !self.mobs[i].extra.effects.is_empty() {
            let m = &self.mobs[i];
            let names: Vec<&'static str> = m
                .extra
                .effects
                .iter()
                .map(|e| e.0.trim_start_matches("minecraft:"))
                .collect();
            let (bounds, mut random) = (m.bounds(), m.random.clone());
            self.effects_on_death(&names, bounds, false, Some(id), Some(&mut random));
            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                m.random = random;
            }
        }
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.removed = true;
        }
    }
    /// `AbstractMinecartContainer.remove` and `AbstractChestBoat.remove`: a
    /// chest or hopper minecart or a chest boat spills what it holds
    /// (`Containers.dropContents`) for every destroying reason.
    fn spill_vehicle(&mut self, t: Target) {
        let id = match t {
            Target::Cart(id) | Target::Boat(id) => id,
            _ => return,
        };
        // What an open menu edited goes back to the vehicle first.
        self.release_vehicle_container(id);
        let held = match t {
            Target::Cart(_) => self
                .carts
                .iter_mut()
                .find(|c| c.id == id)
                .map(|c| (c.position, &mut c.items)),
            _ => self
                .boats
                .iter_mut()
                .find(|b| b.id == id)
                .map(|b| (b.position, &mut b.items)),
        };
        let Some((at, items)) = held else {
            return;
        };
        if items.iter().all(Option::is_none) {
            return;
        }
        let items = std::mem::take(items);
        // `Containers.dropContents(level, entity, container)` passes the
        // exact position, and `dropItemStack` floors it (`Math.floor(x)`):
        // the block position is the same spill.
        let at = BlockPos {
            x: at.x.floor() as i32,
            y: at.y.floor() as i32,
            z: at.z.floor() as i32,
        };
        self.with_level(|_, level, _| level.drop_lost_contents(at, items));
        self.flush_spill();
    }
    /// Takes out the entities mods removed since the last sweep.
    pub(super) fn native_removals_flush(&mut self) {
        if self.persist.removals.is_empty() {
            return;
        }
        for t in std::mem::take(&mut self.persist.removals) {
            match t {
                Target::Crystal(id) => self.crystals.retain(|c| c.id != id),
                Target::Dragon(id) => {
                    if self.dragon.as_ref().is_some_and(|d| d.id == id) {
                        self.dragon = None;
                    }
                }
                Target::Player | Target::Guest(_) | Target::Mob(_) => {}
                // The kinds `/kill` already removes with no death.
                t => {
                    self.spill_vehicle(t);
                    self.kill_target_in_level(t);
                    // A display, marker or interaction isn't saved either.
                    if let Target::Other(id) = t {
                        self.persist.raw.remove(&id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{commands::Target, native_entity_refs::tests::with_game};
    use horizon_client::chunk::Chunk;
    use horizon_world::nbt::{Compound, Tag};
    use std::sync::Arc;

    /// Spawns `kind` at `pos` from `nbt`, as `/summon` loads it: its id.
    fn summon(game: &mut super::Game<'_>, kind: &str, pos: [f64; 3], mut nbt: Compound) -> u32 {
        nbt.insert("id".into(), Tag::String(kind.into()));
        let pos = pos.map(|v| Tag::Double(v.to_bits())).to_vec();
        nbt.insert("Pos".into(), Tag::List(pos));
        match game.spawn_entity_from_nbt(Tag::Compound(nbt)) {
            Some(Target::Other(id) | Target::Boat(id)) => id,
            t => panic!("{kind} spawned as {t:?}"),
        }
    }

    #[test]
    #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
    fn a_removed_handle_is_gone_before_the_sweep_and_a_chest_boat_spills_where_it_stood() {
        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            let marker = summon(game, "minecraft:marker", [2.5, 70.0, 2.5], Compound::new());
            let handle = game.native_entity(marker).unwrap();
            assert!(game.native_entity_discard(handle));
            // `isRemoved` from the call on, though the sweep hasn't run.
            assert!(game.native_entity(marker).is_none());
            assert!(game.others.iter().any(|o| o.id == marker));
            assert!(!game.native_entity_discard(handle));
            game.native_removals_flush();
            assert!(!game.others.iter().any(|o| o.id == marker));
            assert!(!game.persist.raw.contains_key(&marker));

            let mut nbt = Compound::new();
            nbt.insert(
                "Items".into(),
                Tag::List(vec![Tag::compound([
                    ("Slot", Tag::Byte(0)),
                    ("id", Tag::String("minecraft:diamond".into())),
                    ("count", Tag::Int(5)),
                ])]),
            );
            let boat = summon(game, "minecraft:oak_chest_boat", [5.7, 70.2, 9.3], nbt);
            let handle = game.native_entity(boat).unwrap();
            let before = game.dropped.len();
            assert!(game.native_entity_discard(handle));
            assert!(game.native_entity(boat).is_none());
            game.native_removals_flush();
            assert!(!game.boats.iter().any(|b| b.id == boat));
            let spilled = &game.dropped[before..];
            assert_eq!(spilled.iter().map(|i| i.stack.count).sum::<u32>(), 5);
            for item in spilled {
                assert_eq!(&*item.stack.item, "minecraft:diamond");
                // `dropItemStack` floors the boat's position, then spreads
                // the item inside that block.
                assert_eq!(item.position.x.floor(), 5.0);
                assert_eq!(item.position.y.floor(), 70.0);
                assert_eq!(item.position.z.floor(), 9.0);
            }
        });
    }
}
