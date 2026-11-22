//! The collision part of `ServerGamePacketListenerImpl.handlePlayerPositionChange`, through the
//! guest's collision exclusions (arismoko/horizon#3717): a mod's boxes in which the level's blocks don't stop the
//! player are the same boxes its client moves it through.
use super::{BlockCollisionRegistry, Guest, Hub, TickCtx, World};
use crate::collisions::SceneCollisions;
use horizon_core::geometry::{Aabb, ShapeOp, Vec3, VoxelShape};
use horizon_world::{
    block_collision_scan::{BlockCollisionScan, BlockCollisionWorld},
    collision_context::CollisionContext,
    collision_exclusions::Excluding,
    entity_obstruction::EntityIdentity,
    world_movement::{
        CollisionCheckRequest, EntityMovementWorld, MovementRequest, no_world_collision,
        resolve_world_movement,
    },
};
use std::io;

impl Hub {
    /// `handlePlayerPositionChange`'s collision check: moved wrongly, refused,
    /// standing on something, and explicitly overridden by the move gate.
    pub(super) fn collide(
        &self,
        i: usize,
        to: [f64; 3],
        ctx: &TickCtx,
    ) -> (bool, bool, bool, bool) {
        let g = &self.guests[i];
        // noPhysics (spectators) and sleepers move freely.
        let Some(binding) = ctx.collision else {
            return (false, false, false, false);
        };
        if g.spectator() || g.sleeping {
            return (false, false, false, false);
        }
        let scene = SceneCollisions {
            world: ctx.world,
            binding,
            subject: None,
        };
        // A Player-cause move through the move gate (arismoko/horizon#2414); the gate records the movement
        // unchanged and its delta is what the collision resolves.
        let requested = Vec3::new(
            to[0] - g.position[0],
            to[1] - g.position[1],
            to[2] - g.position[2],
        );
        let gate = crate::move_gate::armed(
            &self.loop_context,
            g.entity_id as u32,
            "minecraft:player",
            horizon_world::entity_move::MoverType::Player,
            requested,
        );
        let movement = gate.delta();
        // A listener's decision that is not the request (arismoko/horizon#2414, ruling 5): the
        // flag comes from the gate, not from comparing deltas here — a walk's
        // step is below «moved wrongly»'s threshold, and its cancel must
        // correct the guest all the same.
        let overridden = gate.overridden();
        let (wrongly, refused, stands) =
            check_scene_move(g, to, requested, movement, overridden, scene);
        (wrongly, refused, stands, overridden)
    }

    /// A mod displaced the player while their already-validated walking report
    /// was in the move hook. Validate its translated remainder in the new scene
    /// without firing that hook or the movement gate a second time.
    pub fn recheck_displaced_report(
        &mut self,
        conn: u32,
        world: &World,
        binding: &BlockCollisionRegistry<'_>,
    ) {
        let Some(i) = self.guests.connection_index(conn) else {
            return;
        };
        let g = &self.guests[i];
        let Some(pending) = g.pending_move.as_ref() else {
            return;
        };
        let to = pending.report.to;
        let [x, y, z] = g.position;
        let requested = Vec3::new(to[0] - x, to[1] - y, to[2] - z);
        if g.spectator() {
            return;
        }
        let (_, refused, stands) = check_scene_move(
            g,
            to,
            requested,
            requested,
            false,
            SceneCollisions {
                world,
                binding,
                subject: None,
            },
        );
        self.guests.update(i, |g| {
            if let Some(pending) = g.pending_move.as_mut() {
                if refused {
                    // Keep any look change. Walking cannot be written through
                    // a block just because it was valid before the displacement.
                    pending.report.to = g.position;
                    pending.report.flags = (pending.report.flags & !1) | u8::from(g.on_ground);
                    pending.report.stands = g.rests_on_ground;
                    pending.report.floating = false;
                } else {
                    pending.report.stands = stands;
                    pending.report.floating &= !stands;
                }
            }
        });
    }
}

fn check_scene_move(
    g: &Guest,
    to: [f64; 3],
    requested: Vec3,
    movement: Vec3,
    overridden: bool,
    mut scene: SceneCollisions<'_, '_>,
) -> (bool, bool, bool) {
    if g.collision_exclusions.is_empty() {
        return check_move(g, to, requested, movement, overridden, &mut scene);
    }
    check_move(
        g,
        to,
        requested,
        movement,
        overridden,
        &mut Excluding {
            world: &mut scene,
            boxes: g.collision_exclusions.boxes(),
        },
    )
}

/// [`Hub::collide`] against `world`'s blocks.
fn check_move<W>(
    g: &Guest,
    to: [f64; 3],
    requested: Vec3,
    movement: Vec3,
    overridden: bool,
    world: &mut W,
) -> (bool, bool, bool)
where
    W: BlockCollisionWorld<Error = io::Error> + EntityMovementWorld<Error = io::Error>,
{
    // getBoundingBox: the box of the body's pose (swimming and crawling
    // are 0.6 tall), so a swimmer under a ceiling isn't sent back.
    let d = g.dimensions();
    let (half, height) = (f64::from(d.width) / 2.0, f64::from(d.height));
    let at = |p: [f64; 3]| {
        Aabb::new(
            Vec3::new(p[0] - half, p[1], p[2] - half),
            Vec3::new(p[0] + half, p[1] + height, p[2] + half),
        )
    };
    let (old, new) = (at(g.position), at(to));
    // player.move, then how far from the target it ended (vanilla drops
    // the vertical part: `yDist > -0.5 || yDist < 0.5` always holds).
    // The mover's own sneaking, as CollisionContext.of(player): a sneaking
    // player descends through scaffolding.
    let context = CollisionContext::position(g.position[1]).with_descending(g.sneaking);
    let done = resolve_world_movement(
        world,
        MovementRequest {
            entity: EntityIdentity(0),
            context,
            bounds: old,
            movement,
            max_step: 0.6,
            on_ground: g.on_ground,
            candidate_budget: 1_000_000,
        },
    )
    .ok();
    // The gate decides what actually moves: the requested delta, a listener's
    // replacement, or nothing (a cancel). «Moved wrongly» still measures the
    // **requested** move against what happened, as vanilla measures the
    // packet's target - a cancelled move that went nowhere is the full
    // discrepancy. A decision that is not the request fails whatever the
    // distance (arismoko/horizon#2414, ruling 5): a cancel or a replacement always sends the
    // guest a correction, so a walk's small step cannot slip under the
    // threshold and keep going. Vanilla's exemptions - a creative guest, and
    // the unobstructed-start check below - belong to the distance test alone
    // (review round 1): an overridden move is refused for a creative guest
    // and from an obstructed start too.
    let moved_wrongly = g.checks_wrong_moves()
        && done.is_some_and(|done| {
            let (dx, dz) = (requested.x - done.x, requested.z - done.z);
            dx * dx + dz * dz > 0.0625
        });
    let fail = overridden || moved_wrongly;
    // Entity.move: verticalCollisionBelow, the player "stands on something".
    let stands = done.is_some_and(|done| movement.y < 0.0 && done.y != movement.y);
    if fail {
        let unobstructed = no_world_collision(
            world,
            CollisionCheckRequest {
                entity: EntityIdentity(0),
                context: CollisionContext::position(old.min_position().y),
                bounds: old,
                candidate_budget: 1_000_000,
            },
        );
        // An overridden decision is refused from wherever it starts: the
        // unobstructed-old-box exemption is the distance test's, like
        // creative above (review round 1).
        if overridden || unobstructed.unwrap_or(true) {
            return (true, true, stands);
        }
    }
    // isEntityCollidingWithAnythingNew: a block the old box did not
    // already touch.
    let shrink = f64::from(1.0E-5_f32);
    let shrink = Vec3::new(shrink, shrink, shrink);
    let Ok(old_shape) = VoxelShape::from_box(old.deflate(shrink)) else {
        return (fail, false, stands);
    };
    let Ok(scan) = BlockCollisionScan::new(world, context, new.deflate(shrink), false, 1 << 20)
    else {
        return (fail, false, stands);
    };
    for hit in scan {
        let Ok(hit) = hit else { break };
        if !hit
            .shape
            .join_is_not_empty(&old_shape, ShapeOp::AND)
            .unwrap_or(true)
        {
            return (fail, true, stands);
        }
    }
    (fail, false, stands)
}
