//! Moving and turning an entity for native server mods, through an
//! `EntityRef` (docs/modding/src/how-to/native-mods.md, "Entity and level handles", #7387):
//! teleport, rotation, head yaw, facing, size and pushes. Writes move the
//! entity as `/tp` and `/rotate` do, so clients see it as they see those.
//! Reads return `None`, writes false, for a stale handle or on a client.
use super::entity::{Entity as _, EntityHandle, with_entity_mut};
use super::{
    Dim, Game,
    commands::{Target, entity_pitch},
    vehicle_teleport::TeleportMotion,
};
use horizon_client::{motion::MotionWorld, raw_payloads::Endpoint};
use horizon_core::geometry::Vec3;
use horizon_protocol::player_displacement::{self, Displacement};
use horizon_world::entity_move::MoverType;
use mod_api::{EntityRef, PayloadBytes, PayloadId, PlayerRef};

/// `Level.isInSpawnableBounds` and `/tp`'s limits: 30,000,000 blocks out,
/// 20,000,000 up or down.
pub(super) fn in_bounds([x, y, z]: [f64; 3]) -> bool {
    (-30_000_000.0..30_000_000.0).contains(&x)
        && (-30_000_000.0..30_000_000.0).contains(&z)
        && (-20_000_000.0..20_000_000.0).contains(&y)
}

/// `Direction.fromYRot`: the horizontal direction a yaw faces.
pub(super) fn facing(yaw: f32) -> &'static str {
    const BY_2D: [&str; 4] = ["south", "west", "north", "east"];
    BY_2D[((f64::from(yaw) / 90.0 + 0.5).floor() as i32 & 3) as usize]
}

impl Game<'_> {
    /// `Entity.move(SELF)`: immediate collision-resolved displacement, not a
    /// teleport or an addition to the body's velocity.
    pub(super) fn entity_move_self(&mut self, entity: EntityRef, [x, y, z]: [f64; 3]) -> bool {
        let Some(target) = self.native_entity_target(entity) else {
            return false;
        };
        if self.target_level(target) != self.dim
            || !self
                .target_position(target)
                .is_some_and(|[px, py, pz]| in_bounds([px + x, py + y, pz + z]))
        {
            return false;
        }
        let delta = Vec3::new(x, y, z);
        match target {
            Target::Player => self
                .lan_acting
                .is_some_and(|conn| self.move_acting_player(conn, delta)),
            Target::Guest(conn) => {
                if self
                    .lan
                    .as_ref()
                    .and_then(|h| h.hub.guest(conn))
                    .and_then(|g| g.next_player_displacement())
                    .is_none()
                {
                    return false;
                }
                self.as_guest(conn, |game| game.move_acting_player(conn, delta))
                    .unwrap_or(false)
            }
            _ => {
                if self.passenger_vehicle_target(target).is_some() {
                    return false;
                }
                self.native_walk_entity(entity.id())
                    .is_some_and(|handle| self.move_unridden_entity(handle, delta))
            }
        }
    }

    fn move_acting_player(&mut self, conn: u32, delta: Vec3) -> bool {
        use super::move_gate::MoveWho;
        let Some((sequence, sender)) = self.lan.as_ref().and_then(|h| {
            let guest = h.hub.guest(conn)?;
            Some((guest.next_player_displacement()?, guest.sender.clone()))
        }) else {
            return false;
        };
        let Some(player) = self
            .player
            .as_ref()
            .filter(|p| !p.teleported && !p.health.dead_or_dying())
        else {
            return false;
        };
        if self.riding.is_some() {
            return false;
        }
        let (from, ground, collision) = (player.position, player.ground, player.collision);
        let gate = self.entity_move(MoveWho::Player, MoverType::SelfMovement, delta);
        if gate.skipped() || !in_bounds(from.add_vector(gate.delta()).components()) {
            return false;
        }
        let result = self.with_displacement_motion(|motion| motion.move_player_displacement(&gate));
        let player = self.player.as_mut().unwrap();
        let at = player.position;
        if result.is_err() || !in_bounds(at.components()) {
            // An unsuccessful sweep must not leave the guest turn with a
            // position discrepancy for its teleport handling.
            (player.position, player.ground, player.collision) = (from, ground, collision);
            return false;
        }
        let on_ground = player.ground.on_ground;
        let moved = at.subtract(from);
        if moved == Vec3::ZERO {
            if let Some(host) = self.lan.as_mut() {
                host.hub.update_guest(conn, |g| {
                    g.on_ground = on_ground;
                    g.rests_on_ground = on_ground;
                });
            }
            return true;
        }
        let entity = player.entity_id as u32;
        let result = self.queue_current_payload(
            Endpoint::server(sender.clone()),
            &PayloadId(player_displacement::CHANNEL.into()),
            PayloadBytes(player_displacement::encode(Displacement {
                sequence,
                delta: moved.components(),
            })),
        );
        // Publish the server body's position to tracking immediately, not as
        // a position discrepancy for `leave_guest` to turn into a teleport.
        if let Some(host) = self.lan.as_mut() {
            host.hub.update_guest(conn, |g| {
                g.record_player_displacement(
                    sequence,
                    from.components(),
                    at.components(),
                    on_ground,
                );
            });
        }
        if result.is_err() {
            sender.close();
            return false;
        }
        self.entity_section_moved_id(entity);
        true
    }

    pub(super) fn remote_player_displacement(&mut self, data: &[u8]) -> horizon_client::Result<()> {
        let displacement = player_displacement::decode(data)?;
        let [x, y, z] = displacement.delta;
        let delta = Vec3::new(x, y, z);
        if let Some(player) = self.player.as_ref().filter(|_| self.riding.is_none()) {
            let from = player.position;
            if in_bounds(from.add_vector(delta).components()) {
                self.with_displacement_motion(|motion| motion.predict_player_displacement(delta))?;
            }
            let player = self.player.as_ref().unwrap();
            let (position, id) = (player.position, player.entity_id as u32);
            let moved = position.subtract(from);
            self.previous_position = self.previous_position.add_vector(moved);
            if let Some(remote) = self.remote.as_mut() {
                // The server already knows about this translation, but not any walking
                // the client hasn't reported, or a local obstruction the server didn't see.
                remote.position_sender.displace_baseline(delta);
                if moved.subtract(delta).length_squared() > 1.0e-12 {
                    remote.position_sender.request_position();
                }
            }
            self.entity_section_moved_id(id);
        }
        if self.replay.playing() {
            return Ok(());
        }
        self.send_payload_to_server(
            &PayloadId(player_displacement::CHANNEL.into()),
            PayloadBytes(displacement.sequence.to_be_bytes().to_vec()),
        )
        .map_err(|e| format!("acknowledge player displacement: {e:?}"))?;
        Ok(())
    }

    fn with_displacement_motion<R>(
        &mut self,
        f: impl FnOnce(&mut MotionWorld<'_, '_, '_>) -> R,
    ) -> R {
        let entities = self.player_entity_collisions();
        f(&mut MotionWorld {
            entities: &entities,
            player: self.player.as_mut().expect("checked player body"),
            scene: &mut self.world,
            assets: &self.pack.assets,
            collision_binding: self.collision,
            resources: self.motion,
            health: &self.health_resources,
            loop_context: &self.loop_context,
            damage_hooks: &self.damage_hooks,
            tag_items: Some(world_items!(self)),
            block_registries: Some(&self.pack.block_registries),
        })
    }

    /// Moves the entity to `pos` in its level (`Entity.teleportTo`), as
    /// `/tp` does: a player is placed there and told, a rider gets off. With
    /// `rotation` (yaw, pitch in degrees) it also turns. False for a position
    /// outside the world's bounds, for wall-mounted entities or dragon parts, or when the
    /// requested rotation cannot be stored.
    pub(super) fn entity_teleport_to(
        &mut self,
        entity: EntityRef,
        pos: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> bool {
        let Some(t) = self.teleport_target(entity, pos, rotation) else {
            return false;
        };
        self.teleport_to(t, pos, rotation, true);
        self.teleport_stops(t);
        true
    }
    /// Sends the entity to `pos` in the level `dimension` (`Entity.teleport`
    /// with a portal's transition, `TeleportMotion::Keep`): the level opens if
    /// it isn't, the entity keeps its motion, a player arrives after the
    /// current tick, a vehicle takes its passengers along. In the entity's own
    /// level it is `entity_teleport_to` with the motion kept. False for what
    /// `entity_teleport_to` refuses, a dimension that isn't registered, and
    /// kinds that cannot change level here (`teleport_target_across`).
    pub(super) fn entity_teleport_to_level(
        &mut self,
        entity: EntityRef,
        dimension: &str,
        pos: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> bool {
        let Some(t) = self.teleport_target(entity, pos, rotation) else {
            return false;
        };
        if !Dim::exists(dimension) {
            return false;
        }
        self.teleport_to_level(t, Dim::from_id(dimension), pos, rotation)
    }
    /// A mod sends `t` to `pos` in `level` (`Entity.teleport` with a
    /// `TeleportTransition` that keeps the motion, `TeleportMotion::Keep`): in
    /// its own level when that is `level`, else as `/tp` into another level
    /// does. False for a kind that only moves within its level (armor stands,
    /// falling blocks, ...; `teleport_target_across`), a player who is not a
    /// server guest, or a transfer the game could not make. A fake player
    /// stays, and the call is true.
    pub(in crate::game) fn teleport_to_level(
        &mut self,
        t: Target,
        level: Dim,
        pos: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> bool {
        if self.target_level(t) == level {
            let motion = self.transition_motion(t);
            self.teleport_to(t, pos, rotation, true);
            if let Some(motion) = motion {
                self.restore_transition_motion(t, motion);
            }
            return true;
        }
        self.teleport_across(t, level, pos, rotation, true, TeleportMotion::Keep)
            .unwrap_or(false)
    }
    /// What a teleport of `entity` to `pos` moves, when `/tp` could place it there.
    fn teleport_target(
        &self,
        entity: EntityRef,
        pos: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> Option<Target> {
        let t = self.native_entity_target(entity)?;
        if !in_bounds(pos)
            || matches!(
                t,
                Target::Frame(_) | Target::Painting(_) | Target::DragonPart(_)
            )
            || rotation.is_some_and(|r| {
                !r.iter().all(|v| v.is_finite())
                    || !matches!(
                        t,
                        Target::Player | Target::Guest(_) | Target::Mob(_) | Target::Other(_)
                    )
            })
        {
            return None;
        }
        Some(t)
    }
    /// Turns the entity to `yaw` and `pitch` in degrees (`Entity.setYRot`,
    /// `setXRot`): the pitch wraps and is clamped to -90..90. False for kinds
    /// whose rotation Horizon does not fully store, or an unavailable player.
    /// Does not teleport or dismount.
    pub(super) fn entity_set_rotation(&mut self, entity: EntityRef, yaw: f32, pitch: f32) -> bool {
        let Some(t) = self.native_entity_target(entity) else {
            return false;
        };
        if !yaw.is_finite() || !pitch.is_finite() {
            return false;
        }
        let pitch = entity_pitch(pitch);
        match t {
            Target::Player | Target::Guest(_) => {
                self.for_player(t, |g| {
                    if g.player.is_none() {
                        return false;
                    }
                    g.yaw = yaw;
                    g.pitch = pitch;
                    true
                }) == Some(true)
            }
            Target::Mob(id) => {
                let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
                    return false;
                };
                m.yaw = yaw;
                m.head_pitch = pitch;
                m.needs_sync = true;
                true
            }
            Target::Other(id) => {
                let Some(i) = self.others.iter().position(|o| o.id == id) else {
                    return false;
                };
                self.others.update(i, |o| {
                    o.yaw = yaw;
                    o.pitch = pitch;
                });
                true
            }
            _ => false,
        }
    }
    /// The yaw of the entity's head (`Entity.getYHeadRot`): a mob's or a
    /// player's own, zero for the kinds without a head.
    pub(super) fn entity_head_yaw(&self, entity: EntityRef) -> Option<f32> {
        let t = self.native_entity_target(entity)?;
        if let Some(g) = self.native_target_guest(t) {
            return Some(g.head_yaw);
        }
        if let Target::Mob(id) = t {
            return Some(self.mobs.iter().find(|m| m.id == id)?.head_yaw);
        }
        Some(0.0)
    }
    /// The yaw of a mob's body (`LivingEntity.yBodyRot`), which its model turns
    /// with and its head turns against; `entity_head_yaw` reads the head. None
    /// for the other kinds, whose body yaw Horizon doesn't keep.
    pub(super) fn entity_body_yaw(&self, entity: EntityRef) -> Option<f32> {
        let Target::Mob(id) = self.native_entity_target(entity)? else {
            return None;
        };
        Some(self.mobs.iter().find(|m| m.id == id)?.body_yaw)
    }
    /// Turns a mob's head to `yaw` (`Entity.setYHeadRot`); its body follows
    /// as its AI turns it. Players' heads can turn too; false for other kinds
    /// or an unavailable player.
    pub(super) fn entity_set_head_yaw(&mut self, entity: EntityRef, yaw: f32) -> bool {
        if !yaw.is_finite() {
            return false;
        }
        match self.native_entity_target(entity) {
            Some(Target::Mob(id)) => match self.mobs.iter_mut().find(|m| m.id == id) {
                Some(m) => {
                    m.head_yaw = yaw;
                    m.needs_sync = true;
                    true
                }
                None => false,
            },
            Some(t @ (Target::Player | Target::Guest(_))) => {
                let conn = match t {
                    Target::Player => self.lan_acting,
                    Target::Guest(conn) => Some(conn),
                    _ => None,
                };
                conn.and_then(|conn| {
                    self.lan
                        .as_mut()?
                        .hub
                        .update_guest(conn, |g| g.head_yaw = yaw)
                })
                .is_some()
            }
            _ => false,
        }
    }
    /// The horizontal direction the entity faces (`Entity.getDirection`):
    /// `north`, `south`, `east` or `west`.
    pub(super) fn entity_direction(&self, entity: EntityRef) -> Option<&'static str> {
        let t = self.native_entity_target(entity)?;
        Some(facing(self.target_rotation(t)[0]))
    }
    /// The entity's width and height (`Entity.getBbWidth`, `getBbHeight`):
    /// its box's, so a baby's or a scaled entity's own.
    pub(super) fn entity_dimensions(&self, entity: EntityRef) -> Option<[f64; 2]> {
        let [min, max] = self.entity_bounding_box(entity)?;
        Some([max[0] - min[0], max[1] - min[1]])
    }
    /// Adds `delta` to the entity's velocity (`Entity.push`,
    /// `addDeltaMovement`). False for an unavailable player, an entity
    /// Horizon doesn't simulate, or a value that isn't finite.
    pub(super) fn entity_push(&mut self, entity: EntityRef, delta: [f64; 3]) -> bool {
        if !delta.iter().all(|v| v.is_finite()) {
            return false;
        }
        let Some(target) = self.native_entity_target(entity) else {
            return false;
        };
        if matches!(target, Target::Player | Target::Guest(_)) {
            return self.native_player_motion(target, |v| {
                Vec3::new(v.x + delta[0], v.y + delta[1], v.z + delta[2])
            });
        }
        let Some(h) = self.native_walk_entity(entity.id()) else {
            return false;
        };
        let [x, y, z] = delta;
        with_entity_mut!(self, h, e => {
            let v = e.velocity_mut();
            *v = Vec3::new(v.x + x, v.y + y, v.z + z);
        });
        if let EntityHandle::Mob(i) = h {
            self.mobs[i].needs_sync = true;
        }
        true
    }
    /// Sets a server player's velocity and tells their client
    /// (`ClientboundSetEntityMotionPacket` about themselves, as Paper's
    /// `Player.setVelocity`). False for a stale or unavailable player, an
    /// entity that is not a player, or a value that isn't finite.
    pub(super) fn player_set_velocity(&mut self, player: PlayerRef, velocity: [f64; 3]) -> bool {
        if !velocity.iter().all(|v| v.is_finite()) {
            return false;
        }
        match self.native_entity_target(player.entity()) {
            Some(target @ (Target::Player | Target::Guest(_))) => {
                let [x, y, z] = velocity;
                self.native_player_motion(target, |_| Vec3::new(x, y, z))
            }
            _ => false,
        }
    }
    /// Changes a player's velocity with `change` and sends their client the
    /// result: the acting player's record (`Target::Player`) or a guest's.
    fn native_player_motion(&mut self, target: Target, change: impl FnOnce(Vec3) -> Vec3) -> bool {
        let conn = match target {
            Target::Player => self.lan_acting,
            Target::Guest(conn) => Some(conn),
            _ => None,
        };
        let Some(conn) = conn else {
            return false;
        };
        let Some(host) = self.lan.as_mut() else {
            return false;
        };
        host.hub
            .update_guest(conn, |g| {
                let motion = if matches!(target, Target::Player) {
                    self.player.as_mut()
                } else {
                    g.motion.as_deref_mut()
                };
                let Some(motion) = motion else {
                    return false;
                };
                motion.velocity = change(motion.velocity);
                let v = motion.velocity;
                g.send(
                    &horizon_protocol::packets::play::Clientbound::SetEntityMotion(
                        horizon_protocol::packets::play::SetEntityMotion {
                            id: g.entity_id,
                            velocity: [v.x, v.y, v.z],
                        },
                    ),
                );
                true
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "daily"]
    fn physical_move_adds_to_outstanding_player_walk_and_stops_at_a_wall() {
        use super::super::native_entity_refs::tests::with_client_game;
        use horizon_client::{lan::guest::InboxItem, save::WorldPreset, title::WorldOptions};
        use horizon_core::geometry::Vec3;
        use horizon_protocol::packets::{Empty, configuration::CustomPayload, play};
        use std::time::{Duration, Instant};

        fn near(actual: [f64; 3], expected: [f64; 3]) {
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1.0e-6, "{actual} != {expected}");
            }
        }

        with_client_game(|client| {
            client.settings.render_distance = 2;
            client.settings.simulation_distance = 2;
            client
                .open_integrated(
                    "physical_move",
                    Some("Physical movement"),
                    Some(1),
                    true,
                    Some(&WorldOptions {
                        world_type: WorldPreset::Flat,
                        ..Default::default()
                    }),
                )
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(60);
            while !client.remote.as_ref().is_some_and(|r| r.joined) {
                client.poll_remote().unwrap();
                assert!(Instant::now() < deadline, "client join");
                std::thread::yield_now();
            }
            client.send(play::Serverbound::PlayerLoaded(Empty));
            let integrated = client.integrated.as_ref().unwrap();
            integrated
                .call(|server| {
                    server.set_allow_commands(true);
                    assert!(server.run_command("forceload add 0 0"));
                })
                .unwrap();
            while !integrated
                .call(|server| server.world.chunks.contains_key(&(0, 0)))
                .unwrap()
            {
                assert!(
                    Instant::now() < deadline,
                    "server loaded the fixture's chunk"
                );
                std::thread::yield_now();
            }
            let (conn, id) = client
                .integrated
                .as_ref()
                .unwrap()
                .call(|server| {
                    let g = &server.lan.as_ref().unwrap().hub.guests[0];
                    let conn = g.conn;
                    let id = g.entity_id as u32;
                    assert!(server.run_command("fill 0 99 0 8 99 7 stone"));
                    assert!(
                        server
                            .run_command("fill 3 100 0 5 100 7 minecraft:stone_slab[type=bottom]")
                    );
                    assert!(server.run_command("fill 6 100 0 6 103 7 stone"));
                    let entity = server.entity(id).unwrap();
                    assert!(entity.teleport_to(server, [1.5, 100.0, 1.5], None));
                    (conn, id)
                })
                .unwrap();
            while !client
                .player
                .as_ref()
                .is_some_and(|p| p.position == Vec3::new(1.5, 100.0, 1.5))
                || !client
                    .integrated
                    .as_ref()
                    .unwrap()
                    .call(move |server| {
                        let g = server.lan.as_ref().unwrap().hub.guest(conn).unwrap();
                        g.next_player_displacement().is_some()
                    })
                    .unwrap()
            {
                client.poll_remote().unwrap();
                assert!(Instant::now() < deadline, "client ready for displacement");
                std::thread::yield_now();
            }

            // The client walked on +Z and turned before either +X translation reached it.
            // Hold that pre-displacement packet on the hub so the race is deterministic.
            let own_velocity = Vec3::new(0.0, 0.0, 0.14);
            let player = client.player.as_mut().unwrap();
            player.position.z += 0.25;
            player.velocity = own_velocity;
            (client.yaw, client.pitch) = (-24.0, 19.0);
            client
                .integrated
                .as_ref()
                .unwrap()
                .call(move |server| {
                    let entity = server.entity(id).unwrap();
                    let host: &mut dyn mod_api::ServerHost = server;
                    assert!(entity.move_self(host, [1.0, 0.0, 0.0]));
                    near(entity.position(host).unwrap(), [2.5, 100.0, 1.5]);
                    assert!(entity.move_self(host, [1.0, 0.0, 0.0]));
                    near(entity.position(host).unwrap(), [3.5, 100.5, 1.5]);
                    assert_eq!(entity.on_ground(host), Some(true));
                    server.lan.as_mut().unwrap().hub.update_guest(conn, |g| {
                        // Sequence 2 cannot acknowledge sequence 1 or remove its translation.
                        g.inbox.push_back(InboxItem::Packet(
                            play::Serverbound::CustomPayload(CustomPayload {
                                channel: super::player_displacement::CHANNEL.into(),
                                data: 2_u64.to_be_bytes().to_vec(),
                            }),
                            None,
                        ));
                        g.inbox.push_back(InboxItem::Packet(
                            play::Serverbound::MovePlayerPosRot(play::MovePlayerPosRot {
                                x: 1.5,
                                y: 100.0,
                                z: 1.75,
                                yaw: -24.0,
                                pitch: 19.0,
                                flags: 1,
                            }),
                            None,
                        ));
                        g.inbox.push_back(InboxItem::Packet(
                            play::Serverbound::ClientTickEnd(Empty),
                            None,
                        ));
                    });
                    server.tick_lan_packets();
                    let g = server.lan.as_ref().unwrap().hub.guest(conn).unwrap();
                    near(g.position, [3.5, 100.5, 1.75]);
                    assert_eq!((g.yaw, g.pitch), (-24.0, 19.0));
                    assert!(g.awaiting_teleport.is_none());
                })
                .unwrap();

            while !client.player.as_ref().is_some_and(|p| p.position.x >= 3.5) {
                client.poll_remote().unwrap();
                assert!(
                    Instant::now() < deadline,
                    "client received both translations"
                );
                std::thread::yield_now();
            }
            let player = client.player.as_ref().unwrap();
            near(player.position.components(), [3.5, 100.5, 1.75]);
            assert_eq!(player.velocity, own_velocity);
            assert_eq!((client.yaw, client.pitch), (-24.0, 19.0));
            assert!(player.ground.on_ground);

            // The client's real acknowledgement is sent before this new-frame walking report.
            // Its already translated X/Y must not acquire another copy of either move.
            client.player.as_mut().unwrap().position.z = 2.0;
            client.send(play::Serverbound::MovePlayerPos(play::MovePlayerPos {
                x: 3.5,
                y: 100.5,
                z: 2.0,
                flags: 1,
            }));
            client.send(play::Serverbound::ClientTickEnd(Empty));
            while !client
                .integrated
                .as_ref()
                .unwrap()
                .call(move |server| {
                    let g = server.lan.as_ref().unwrap().hub.guest(conn).unwrap();
                    assert!(g.awaiting_teleport.is_none());
                    g.position == [3.5, 100.5, 2.0]
                })
                .unwrap()
            {
                assert!(Instant::now() < deadline, "new-frame walk accepted");
                std::thread::yield_now();
            }

            // This time the player's own predicted walk is towards the wall. Its old
            // report cannot be translated all the way; neither body may pass the wall.
            client.player.as_mut().unwrap().position.x += 0.25;
            (client.yaw, client.pitch) = (-48.0, 22.0);
            client
                .integrated
                .as_ref()
                .unwrap()
                .call(move |server| {
                    let entity = server.entity(id).unwrap();
                    assert!(entity.move_self(server, [4.0, 0.0, 0.0]));
                    near(entity.position(server).unwrap(), [5.7, 100.5, 2.0]);
                    assert!(entity.move_self(server, [1.0, 0.0, 0.0]));
                    near(entity.position(server).unwrap(), [5.7, 100.5, 2.0]);
                    server.lan.as_mut().unwrap().hub.update_guest(conn, |g| {
                        g.inbox.push_back(InboxItem::Packet(
                            play::Serverbound::MovePlayerPosRot(play::MovePlayerPosRot {
                                x: 3.75,
                                y: 100.5,
                                z: 2.0,
                                yaw: -48.0,
                                pitch: 22.0,
                                flags: 1,
                            }),
                            None,
                        ));
                    });
                    server.tick_lan_packets();
                    let g = server.lan.as_ref().unwrap().hub.guest(conn).unwrap();
                    near(g.position, [5.7, 100.5, 2.0]);
                    assert_eq!((g.yaw, g.pitch), (-48.0, 22.0));
                    assert!(g.awaiting_teleport.is_none());

                    let kind = server
                        .res
                        .mob_types()
                        .get("minecraft:zombie")
                        .unwrap()
                        .clone();
                    let mob_id = server.take_entity_id();
                    let mut mob = horizon_client::mobs::Mob::new(
                        mob_id,
                        kind,
                        Vec3::new(1.5, 100.0, 3.5),
                        90.0,
                    );
                    mob.on_ground = true;
                    mob.no_ai = true;
                    server.mobs.push(mob);
                    let mob = server.entity(mob_id).unwrap();
                    assert!(mob.move_self(server, [2.0, 0.0, 0.0]));
                    near(mob.position(server).unwrap(), [3.5, 100.5, 3.5]);
                    assert_eq!(mob.on_ground(server), Some(true));
                    assert_eq!(mob.rotation(server).unwrap()[0], 90.0);
                    assert!(!mob.move_self(server, [f64::NAN, 0.0, 0.0]));
                    let handle = server.native_walk_entity(mob_id).unwrap();
                    server.discard_entity(handle);
                    assert!(!mob.move_self(server, [1.0, 0.0, 0.0]));
                })
                .unwrap();
            while !client.player.as_ref().is_some_and(|p| p.position.x > 5.0) {
                client.poll_remote().unwrap();
                assert!(
                    Instant::now() < deadline,
                    "client received wall-clipped translation"
                );
                std::thread::yield_now();
            }
            near(
                client.player.as_ref().unwrap().position.components(),
                [5.7, 100.5, 2.0],
            );
            assert_eq!((client.yaw, client.pitch), (-48.0, 22.0));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn moves_and_pushes_a_live_mob() {
        use super::super::native_entity_refs::tests::with_game;
        use horizon_client::mobs::Mob;
        use horizon_core::geometry::Vec3;

        with_game(|game| {
            let kind = game
                .res
                .mob_types()
                .get("minecraft:zombie")
                .unwrap()
                .clone();
            let mut mob = Mob::new(12, kind, Vec3::ZERO, 0.0);
            mob.ai.nav.path = Some(horizon_client::mobs::path::Path {
                nodes: Vec::new(),
                next: 0,
                target: [1, 64, 1],
                dist_to_target: 0.0,
                reached: true,
            });
            mob.extra.mount = Some(42);
            mob.extra.passenger = true;
            mob.fall_distance = 5.0;
            mob.velocity.y = -2.0;
            game.mobs.push(mob);
            let entity = game.entity(12).unwrap();
            assert!(entity.teleport_to(game, [5.0, 70.0, -3.0], Some([90.0, 120.0])));
            let mob = &game.mobs[0];
            assert!(mob.ai.nav.path.is_none());
            assert!(mob.extra.mount.is_none());
            assert!(!mob.extra.passenger);
            assert_eq!(mob.fall_distance, 5.0);
            assert_eq!(mob.velocity.y, 0.0);
            assert_eq!(entity.position(game), Some([5.0, 70.0, -3.0]));
            assert_eq!(entity.rotation(game), Some([90.0, 90.0]));
            assert_eq!(entity.head_yaw(game), Some(90.0));
            game.mobs[0].body_yaw = 33.0;
            assert_eq!(entity.body_yaw(game), Some(33.0));
            assert_eq!(entity.head_yaw(game), Some(90.0));
            let before = entity.velocity(game).unwrap();
            assert!(entity.set_rotation(game, 15.0, 360.0));
            assert_eq!(entity.rotation(game), Some([15.0, 0.0]));
            assert_eq!(entity.head_yaw(game), Some(90.0));
            assert_eq!(entity.position(game), Some([5.0, 70.0, -3.0]));
            assert_eq!(entity.velocity(game), Some(before));
            assert!(entity.teleport_to(game, [5.0, 70.0, -3.0], Some([0.0, 450.0])));
            assert_eq!(entity.rotation(game), Some([0.0, 90.0]));
            assert!(entity.push(game, [1.0, 2.0, -1.0]));
            assert_eq!(
                entity.velocity(game),
                Some([before[0] + 1.0, before[1] + 2.0, before[2] - 1.0])
            );
            assert!(!entity.teleport_to(game, [f64::NAN, 0.0, 0.0], None));
            assert!(!entity.push(game, [0.0, f64::INFINITY, 0.0]));
            game.armor_stands
                .push(horizon_client::entities::ArmorStand::new(
                    13,
                    Vec3::ZERO,
                    0.0,
                ));
            let stand = game.entity(13).unwrap();
            assert!(!stand.set_rotation(game, 30.0, 10.0));
            game.others.push(super::super::commands::OtherEntity {
                attachment_lifetime: Default::default(),
                knot_check: None,
                eye: None,
                join: Default::default(),
                item_transformation: None,
                interaction: None,
                id: 14,
                kind: "minecraft:marker".into(),
                position: Vec3::ZERO,
                yaw: 60.0,
                pitch: 0.0,
            });
            let marker = game.entity(14).unwrap();
            assert_eq!(marker.head_yaw(game), Some(0.0));
            assert!(!marker.set_head_yaw(game, 10.0));
            game.paintings.push(horizon_client::entities::Painting {
                attachment_lifetime: Default::default(),
                join: Default::default(),
                id: 15,
                pos: [0, 64, 0],
                facing: 0,
                variant: "minecraft:kebab".into(),
                width: 1,
                height: 1,
                check_interval: 0,
            });
            let painting = game.entity(15).unwrap();
            assert!(!painting.teleport_to(game, [1.0, 64.0, 0.0], None));
            assert!(!painting.set_rotation(game, 30.0, 10.0));
        });
    }

    #[test]
    fn a_yaw_faces_vanillas_direction() {
        assert_eq!(super::facing(0.0), "south");
        assert_eq!(super::facing(90.0), "west");
        assert_eq!(super::facing(180.0), "north");
        assert_eq!(super::facing(-90.0), "east");
        assert_eq!(super::facing(-45.0), "south");
        assert_eq!(super::facing(-46.0), "east");
        assert_eq!(super::facing(720.0 + 170.0), "north");
    }

    #[test]
    fn positions_stay_inside_the_worlds_bounds() {
        assert!(super::in_bounds([29_999_999.0, -64.0, 0.0]));
        assert!(super::in_bounds([
            -30_000_000.0,
            -20_000_000.0,
            -30_000_000.0
        ]));
        assert!(!super::in_bounds([30_000_000.0, 64.0, 0.0]));
        assert!(!super::in_bounds([0.0, 20_000_001.0, 0.0]));
        assert!(!super::in_bounds([f64::NAN, 64.0, 0.0]));
    }
    /// Independent vanilla outcomes for spawn bounds and the yaw-to-direction conversion.
    #[test]
    fn move_boundaries_match_vanilla() {
        use serde_json::Value;
        let cases = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/native_entity_moves.jsonl"
        ));
        let expected = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/native_entity_moves_expected.jsonl"
        ));
        let rows = |text: &str| {
            text.lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .collect::<Vec<_>>()
        };
        let (cases, expected) = (rows(cases), rows(expected));
        assert_eq!(cases.len(), expected.len());
        assert!(cases.len() >= 20);
        for (input, expected) in cases.iter().zip(&expected) {
            assert_eq!(input["name"], expected["name"]);
            let pos = std::array::from_fn(|i| input["pos"][i].as_f64().unwrap());
            let yaw = input["yaw"].as_f64().unwrap() as f32;
            assert_eq!(
                super::in_bounds(pos),
                expected["spawnable"].as_bool().unwrap(),
                "{}",
                input["name"]
            );
            assert_eq!(
                super::facing(yaw),
                expected["direction"].as_str().unwrap(),
                "{}",
                input["name"]
            );
        }
    }
}
