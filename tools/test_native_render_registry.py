#!/usr/bin/env python3
r"""Prove native meshing registrations cross the installed host/patch boundary (#19713).

    python3 tools/test_native_render_registry.py LAUNCHER SDK_BASE SCRATCH RESOURCES \
        --target x86_64-unknown-linux-gnu

On Windows run from Git Bash and use --target x86_64-pc-windows-gnu. On Linux
use a display or vscreen. SDK_BASE must contain one signed sdk-<commit> folder
with the exact client SDK for the selected host; use the corresponding launcher
(release-flow.md). The scratch installer pins the source manifest to that SDK.
RESOURCES is a prepared private client version folder. No game rebuild or
player instance is used, and logs are kept in SCRATCH.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

try:
    from . import agent_control_e2e, check_imports
except ImportError:
    import agent_control_e2e
    import check_imports

ROOT = Path(__file__).resolve().parents[1]
MOD = "fnr_render_registry"
PREFIX = "render-registry: "

CARGO = '''[package]
name = "fnr-render-registry"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
horizon-mod-api = { path = ".horizon/sdk/api/crates/horizon-mod-api" }
'''
MANIFEST = '''id = "fnr_render_registry"
name = "Native render registry canary"
version = "0.1.0"
api = "0.2"
side = "client"
capabilities = ["registries"]

[native]
game = "26.3"
horizon = "HEAD"
mixins = "MIXINS"

'''
# Replace the functions that publish the registries, not only the event
# dispatchers they call. Otherwise the host would still own the publications.
WRITERS = (("register_color_resolvers", "resolver"),
           ("register_block_tint_sources", "tint"),
           ("add_section_geometry", "geometry"))
MIXIN = '''
[[native.mixin]]
id = "{writer}"
kind = "HEAD"
target = "horizon_client::game::mods::{writer}"
helper = "src/mixins/{writer}.rs"
'''
PATCH = '''fn enter_{writer}() {{
    eprintln!("render-registry: patch entered {writer}");
}}
'''
RUST = '''use mod_api::{
    AddSectionGeometryEvent, BiomeColorResolver, BlockAndTintGetter, BlockMetadata, BlockState,
    BlockTintSource, ColorResolver, Hooks, RegisterBlockTintSourcesEvent,
    RegisterColorResolversEvent, RegistrationContext, ResolverBiome, SectionGeometry,
    SectionGeometryContext, SectionQuad, TintLayer,
};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

const FIRST: [i32; 3] = [4, 201, 4];
const SECOND: [i32; 3] = [7, 201, 4];
const RGB: u32 = 0x12_AB34;

#[derive(Default)]
struct Mod(OnceLock<ColorResolver>);

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_color_resolvers(
        &self,
        _: &RegistrationContext,
        event: &mut RegisterColorResolversEvent<'_>,
    ) {
        assert!(self.0.set(event.resolvers.register(Arc::new(Resolver::default()))).is_ok());
        eprintln!("render-registry: event registered resolver");
    }

    fn register_block_tint_sources(
        &self,
        _: &RegistrationContext,
        event: &mut RegisterBlockTintSourcesEvent<'_>,
    ) {
        let resolver = *self.0.get().expect("colour resolvers register before block tint");
        event.sources.register(
            vec![TintLayer::Source(Arc::new(GrassTint(resolver)))],
            vec!["minecraft:grass_block".into()],
        );
        eprintln!("render-registry: event registered tint");
    }

    fn add_section_geometry(&self, _: &RegistrationContext, event: &mut AddSectionGeometryEvent<'_>) {
        event.renderers.register(Arc::new(Marker));
        eprintln!("render-registry: event registered geometry");
    }
}

#[derive(Default)]
struct Resolver(AtomicBool);
impl BiomeColorResolver for Resolver {
    fn color(&self, biome: &str, _: &ResolverBiome, _: f64, _: f64) -> u32 {
        if !self.0.swap(true, Ordering::Relaxed) {
            eprintln!("render-registry: biome resolver sampled {biome}");
        }
        RGB
    }
}

struct GrassTint(ColorResolver);
impl BlockTintSource for GrassTint {
    fn color(&self, _: &BlockState, _: &dyn BlockMetadata) -> u32 {
        0xFFFF_FFFF
    }

    fn color_in_world(
        &self,
        _: &BlockState,
        _: &dyn BlockMetadata,
        level: &dyn BlockAndTintGetter,
        pos: [i32; 3],
    ) -> u32 {
        let color = level.block_tint(pos, self.0);
        if pos == FIRST || pos == SECOND {
            eprintln!("render-registry: world tint {pos:?} = {color:08X}");
        }
        color
    }
}

struct Marker;
impl SectionGeometry for Marker {
    fn add(&self, origin: [i32; 3], context: &mut dyn SectionGeometryContext) {
        if origin != [0, 192, 0] {
            return;
        }
        // The runner clears this row, then moves one block from FIRST to SECOND.
        // Reading the mesh snapshot rules out a late callback for the old mesh.
        let air = context.level().block_state([5, 201, 4]).block();
        let first = context.level().block_state(FIRST).block() != air;
        let second = context.level().block_state(SECOND).block() != air;
        let mask = u8::from(first) + 2 * u8::from(second);
        let added = context.add("minecraft:block/gold_block", SectionQuad {
            positions: [[4.0, 10.0, 4.0], [4.0, 10.0, 6.0], [6.0, 10.0, 6.0], [6.0, 10.0, 4.0]],
            uvs: [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]],
            color: 0xFFFF_FFFF,
            light: [15, 0],
            translucent: false,
        });
        eprintln!("render-registry: section {origin:?} mask={mask} quad={added}");
    }
}
'''


def stage_mod(source):
    fixture = source / "render_registry"
    (fixture / "src" / "mixins").mkdir(parents=True, exist_ok=True)
    manifest = MANIFEST
    for writer, _ in WRITERS:
        manifest += MIXIN.format(writer=writer)
        (fixture / "src" / "mixins" / f"{writer}.rs").write_text(
            PATCH.format(writer=writer), encoding="utf-8")
    for name, contents in (("Cargo.toml", CARGO), ("horizon-mod.toml", manifest),
                           ("src/lib.rs", RUST)):
        (fixture / name).write_text(contents, encoding="utf-8")


def check_install(engine):
    record = json.loads((engine / "native-mods.json").read_text(encoding="utf-8"))
    rows = record["libraries"]
    patch, = [engine / row["file"] for row in rows if row["kind"] == "patch"]
    if record["route"] != "prebuilt" or not any(
        row["kind"] == "mod" and row["mod"] == MOD for row in rows
    ):
        raise RuntimeError(f"not the installed engine route with {MOD}: {engine}")
    if sys.platform == "win32":
        host = engine / "horizon_mod_host.dll"
        imports = lambda path: set(map(str.lower, check_imports.pe_imports(path.read_bytes())))
    elif sys.platform.startswith("linux"):
        host = engine / "libhorizon_mod_host.so"
        imports = lambda path: set(check_imports.elf_needs(path.read_bytes())[1])
    else:
        return
    shared = [name for name in imports(host) & imports(patch)
              if Path(name).name.removeprefix("lib").startswith("mod_vocab")]
    if len(shared) != 1 or not (engine / Path(shared[0]).name).is_file():
        raise RuntimeError(f"expected one installed mod_vocab imported by {host.name} and {patch.name}: {shared}")
    print(f"{host.name} and {patch.name} both import {shared[0]}", flush=True)


def wait_for(game, timeout, reason, condition):
    deadline = time.monotonic() + timeout
    while game.poll() is None:
        result = condition()
        if result:
            return result
        if time.monotonic() >= deadline:
            raise RuntimeError(f"timed out waiting for {reason}")
        time.sleep(0.2)
    raise RuntimeError(f"client exited with status {game.returncode} before {reason}")


def exercise(control, game, log):
    wait_for(game, 300, "control endpoint", control.is_file)
    endpoint = json.loads(control.read_text(encoding="utf-8"))
    with socket.create_connection(("127.0.0.1", endpoint["port"]), timeout=320) as conn:
        with conn.makefile("rb") as replies:
            def send(line):
                conn.sendall((line + "\n").encode())
                data = replies.readline()
                if not data:
                    raise RuntimeError("game closed the control connection")
                reply = json.loads(data)
                if "error" in reply:
                    raise RuntimeError(f"{line.split()[0]}: {reply['error']}")
                return reply

            def marks():
                return [line.split(PREFIX, 1)[1] for line in
                        log.read_text(encoding="utf-8", errors="replace").splitlines() if PREFIX in line]

            def meshed(mask, start, *extra):
                required = {f"section [0, 192, 0] mask={mask} quad=true", *extra}
                wait_for(game, 90, ", ".join(sorted(required)), lambda: required <= set(marks()[start:]))

            send(f"auth {endpoint['token']}")
            send("wait-for loaded --timeout 300")
            registration = [mark for writer, event in WRITERS
                            for mark in (f"patch entered {writer}", f"event registered {event}")]
            if [mark for mark in marks() if mark in registration] != registration:
                raise RuntimeError(f"did not see patch and one registration of each callback: {marks()}")

            send("command tp @s 0.5 201 0.5 -45 0")
            wait_for(game, 60, "origin chunk", lambda: send("block 4 201 4")["loaded"])
            start = len(marks())
            send("command fill 0 200 0 8 200 5 minecraft:stone")
            send("command fill 4 201 4 7 201 4 minecraft:air")
            send("command tp @s 0.5 201 0.5 -45 0")
            meshed(0, start)

            start = len(marks())
            send("command setblock 4 201 4 minecraft:grass_block")
            meshed(1, start, "world tint [4, 201, 4] = FF12AB34")
            start = len(marks())
            send("command setblock 4 201 4 minecraft:air")
            send("command setblock 7 201 4 minecraft:grass_block")
            meshed(2, start, "world tint [7, 201, 4] = FF12AB34")
            if not any(mark.startswith("biome resolver sampled minecraft:") for mark in marks()):
                raise RuntimeError("no biome sample from the registered resolver")
            send("quit")


def play(engine, work, resources):
    work.mkdir()
    (work / "options.txt").write_text("onboardAccessibility:false\n", encoding="utf-8")
    private = work / "control"
    private.mkdir(mode=0o700)
    control = private / "native-render.json"
    executable = engine / ("horizon-client.exe" if sys.platform == "win32" else "horizon-client")
    cmd = [str(executable), "--game-dir", str(work), "--resources", str(resources),
           "--control-socket", str(control), "--offscreen", "--no-pause",
           "--world", "RenderRegistry", "--seed", "5", "--creative",
           "--render-distance", "4", "--timeout-seconds", "720"]
    env = agent_control_e2e.game_env(os.environ.get("VSCREEN", "native-render-registry"), home=work)
    env.update(HORIZON_LAUNCHER_HOME=str(work / "launcher"), XDG_DATA_HOME=str(work / "data"),
               XDG_CONFIG_HOME=str(work / "config"), HORIZON_TEST_LOOPBACK="1",
               HORIZON_LAUNCHER_TRASH=str(work / "trash"),
               HORIZON_LAUNCHER_CLIPBOARD=str(work / "clipboard"),
               HORIZON_LAUNCHER_DESKTOP=str(work / "desktop"), HORIZON_TELEMETRY_URL="off")
    log = work / "client.log"
    print(f"Client log: {log}", flush=True)
    with log.open("wb") as output, subprocess.Popen(
        cmd, cwd=work, env=env, stdout=output, stderr=subprocess.STDOUT
    ) as game:
        try:
            exercise(control, game, log)
            if game.wait(60):
                raise RuntimeError(f"client exited with status {game.returncode}")
        finally:
            if game.poll() is None:
                game.kill()
    print("Native patch registration and host section geometry, tint, and biome resolver passed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    for name in ("launcher", "sdk_base", "scratch", "resources"):
        parser.add_argument(name, type=lambda path: Path(path).resolve())
    parser.add_argument("--target", required=True, help="exact SDK target for this machine")
    args = parser.parse_args()
    if not (args.resources / "horizon-resources.json").is_file():
        parser.error("RESOURCES must be the private prepared client version folder")
    source = args.scratch / "source"
    build = args.scratch / "install"
    stage_mod(source)
    env = dict(os.environ, FNR_SIDE="client", FNR_MODS="render_registry", FNR_ROUTE="prebuilt",
               FNR_MOD_ROOT=source.as_posix(), FNR_TARGET=args.target, HORIZON_TELEMETRY_URL="off")
    subprocess.run([
        "bash", "tools/ci/fn_replace_check.sh", args.launcher.as_posix(),
        args.sdk_base.as_posix(), build.as_posix(),
    ], cwd=ROOT, env=env, check=True)
    engine = build / "engine"
    check_install(engine)
    play(engine, build / "run", args.resources)


if __name__ == "__main__":
    main()
