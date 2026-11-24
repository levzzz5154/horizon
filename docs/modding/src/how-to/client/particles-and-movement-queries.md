# Client particles and movement queries

## Client particles

`ClientHost::add_particle` submits the API's `ParticleOptions`, a world position and
velocity to the ordinary client particle queue. The client must have a loaded player
and world. Non-finite numeric inputs are refused. A successful submission does not
promise a visible particle: provider availability, sprite sets, distance and the
player's particle settings apply. This method is not available on `ServerHost`.

Particle options carry a `ParticleKey`, not an engine particle enum or a provider name.
Request the key with `Registrar::particle_key` during registration and retain it. The game resolves
provider identities once when content loads. `ParticleOptions::simple(key)` handles both vanilla
and custom sprite providers; custom providers require an `assets/<namespace>/particles/<path>.json`
sprite set. All option constructors take the resolved key, including `dust`, `item`, `trail`,
`vibration`, `shriek`, `sculk_charge` and `dust_transition`. Use the corresponding vanilla key
for these parameter families. Sending an option performs no string parsing or registry scan.

For stack-derived breaking or eating particles, call
`ClientHost::item_particle_options(&stack)` and pass the returned options to
`add_particle`. This read resolves the ground model with seed 0, preserving all
layer particle sprites and resource-pack and stack-component overrides. It returns
`None` for invalid stacks or unavailable resources/provider. It is available in
Setup, Reloading and World, on native and wasm. Resolve again after resource reloads.
It allocates a sprite list only when called; it adds no per-tick work.

### Tick and hurt triggers

Subscribe to `client_tick_post` for continuous effects or to observe a local sprint
transition. The hook also runs while paused and on the title screen; check
`host.is_paused()` and `host.local_player()` before sampling the world. For example,
this listener emits one particle when the local player starts sprinting during rain.
`self.last_sprint` is a `Mutex<Option<(EntityRef, bool)>>` initially containing `None`;
`self.spark` is a `ParticleKey` retained from registration (try `minecraft:flame`).

```rust
fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
    let current = host.local_player().filter(|_| !host.is_paused()).and_then(|player| {
        let entity = player.entity();
        Some((entity, entity.is_sprinting(host)?))
    });
    let previous = std::mem::replace(&mut *self.last_sprint.lock().unwrap(), current);
    let Some((entity, true)) = current else { return };
    if previous != Some((entity, false)) || host.graphics_options().particles == 2 {
        return;
    }
    let Some(level) = entity.level(host) else { return };
    let Some((rain, _)) = level.weather(host) else { return };
    if rain <= 0.2 || entity.eye_fluid(host) != Some(None) {
        return;
    }
    let Some([x, y, z]) = entity.position(host) else { return };
    host.add_particle(&ParticleOptions::simple(self.spark), [x, y + 0.1, z], [0.0; 3]);
}
```

The first sample after joining, replacing the player or resuming from pause sets a
baseline; a held sprint does not repeat the effect. Update the baseline before effect
gates so changes in weather or configuration do not invent a sprint start. For a
continuous sprint effect, run the mod's rate limit while sprint is true instead.
The sprint read is local-player-only on a client.

For water ripples, take a bounded batch of loaded columns with
`host.client_surface_heights(&columns, Heightmap::MotionBlocking)`, then check the
surface blocks with `level.block(host, pos)` and block properties with
`level.block_property(host, pos, "level")` (ordinary water with level `"0"` is a source).
Use `level.client_biome(host, pos)` for the biome and `level.biome_properties(host, &biome)`
for its climate, skipping unavailable samples. A global rain level above `0.2` does
not mean every biome or height gets rain; `LevelRef::is_raining_at` is a server read.
For held-item effects, `host.living(player.entity()).and_then(|p| p.hand_item(host, Hand::Main))`
reads the local stack. Distinguish real enchantments from a forced glint by checking for
a nonempty `minecraft:enchantments` compound via `ItemResource::of(&stack).value(host, &component_id)`.

Use `client_living_hurt` for one trigger per received damage notice for a tracked living
victim (player, mob, or dragon), not a tick-polled hurt timer. The event has `entity`,
`causing_entity`, and `direct_entity`. Some sources
are absent or not tracked by the client; skip them if the effect needs an attacker.
For a local-attacker effect compare `causing_entity` with the local player's `EntityRef`
and use the local hand read above. The event is not a server damage log and cannot
recover damage the server did not send to this client. Separate hurt-tilt packets do not
produce another trigger. Apply the same pause, player and configuration guards, then
read `event.entity.position(host)` and submit the effect there. A per-victim cooldown,
if desired, belongs to the mod.

Each `add_particle` call submits one particle, only on this client. Use a bounded loop
for a burst. `ParticleOptions::dust` takes a scale for the dust provider; there is no
universal scale override for `simple`. A custom provider chooses its own sprite size.

## Client fireworks

`ClientHost::create_fireworks` draws a firework's explosions from a `FireworksRequest`: the
position, the rocket's velocity carried by the sparks, a list of `FireworkExplosion`s (shape,
colours, fade colours, trail, twinkle) and `audible`. It uses the starter a bursting rocket
uses, so the explosions follow each other every other tick as a rocket's do. With `audible`
false the sparks and flash draw but the blast and twinkle sounds don't play, as for a
silent rocket. Nothing reaches the server or other players: a mod's server half tells each
client half to draw it. The request is refused on a server, without a loaded client player
and world, for non-finite numbers and for more than 256 explosions.

## Bot movement and block collisions

`PlayerRef::simulate_move` predicts one server player's movement tick from
`PlayerInput`, using the game's movement implementation without replacing the
live player's body. It returns `SimulatedMove` with position, velocity, ground,
horizontal-collision and sprinting results. It refuses stale or inaccessible players,
dead players, spectators, riders and invalid numeric input. A supplied velocity is
input to the prediction, not a write to the player.

`LevelRef::collision_boxes` reads block collision boxes within a finite, ordered
axis-aligned box. It excludes entity and border collisions. A refused level or a
query covering more than 1,000,000 cells returns `None`; a valid query with no
collisions returns `Some` containing an empty vector. Keep queries bounded rather
than scanning a whole level each tick.

`LevelRef::block_shape(host, state, pos, kind)` reads an explicit `BlockState`'s
outline or collision boxes with an empty context at a position in that level.
The supplied state can differ from the world's block, without changing it;
level-dependent shape inputs come from the actual level. To query the
stored block, first read `level.block_state(host, pos)` and pass that state.
This server read returns block-local boxes, or `None` for a
stale handle, closed level or client. Parked levels are supported.
