#!/usr/bin/env python3
"""Check deterministic chunk_dump digests across noise presets and data packs.

Requires prepared Horizon resources, but does not run Java or use a game jar.
Use --record to print a replacement corpus after an intentional generator change.
"""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import subprocess

import lab
from test_chunk_generation_parity import rust_binary


FIXTURES = lab.ROOT / 'tools/fixtures'
CORPUS = FIXTURES / 'chunk_preset_corpus.json'
SITES = ('0,0,0,0', '40,-24,40,-24')
SEEDS = (12345, 4242)
PRESETS = ('normal', 'amplified', 'large_biomes', 'single_biome_surface')
PACKS = {'overlay': FIXTURES / 'datapacks', 'custom': FIXTURES / 'chunk_preset_datapacks'}
DUMP_OPTIONS = (
    'HORIZON_PRESET', 'HORIZON_DATAPACKS', 'HORIZON_FLAT', 'HORIZON_SCAN', 'HORIZON_LIST_ORDER',
    'HORIZON_SAVED_STARTS', 'HORIZON_OLD_CHUNKS', 'HORIZON_ORDER', 'HORIZON_PIECES',
)


def cases():
    for preset, seed, packs in itertools.product(PRESETS, SEEDS, ('vanilla', 'overlay')):
        yield f'minecraft:{preset}', seed, packs
    for seed in SEEDS:
        yield 'horizon_test:fixed_desert', seed, 'custom'


def dump(binary, resources, preset, seed, packs, dimension='minecraft:overworld'):
    env = {key: value for key, value in os.environ.items() if key not in DUMP_OPTIONS}
    env['HORIZON_PHASE'] = 'terrain'
    if preset is not None:
        env['HORIZON_PRESET'] = preset
    if packs in PACKS:
        env['HORIZON_DATAPACKS'] = str(PACKS[packs])
    result = subprocess.run(
        [str(binary), 'chunk_dump', str(resources), str(seed), dimension, *SITES],
        capture_output=True, env=env, cwd=lab.ROOT,
    )
    if result.returncode:
        raise SystemExit(f'chunk_dump {preset}/{seed}/{packs}: {result.stderr.decode(errors="replace")}')
    return result.stdout


def digest(dump_text):
    """Hash coordinates, all block-state rows and all quart-biome rows in dump order."""
    rows = [row for row in dump_text.splitlines(keepends=True) if row.startswith((b'chunk ', b'q ', b'y '))]
    quart = [row for row in rows if row.startswith(b'q ')]
    blocks = [row for row in rows if row.startswith(b'y ')]
    if sum(row.startswith(b'chunk ') for row in rows) != len(SITES) or not quart or not blocks:
        raise SystemExit('chunk_dump did not emit terrain and biomes for the requested sites')
    biomes = sorted({biome.decode() for row in quart for biome in row.split()[2:]})
    return {'sha256': hashlib.sha256(b''.join(rows)).hexdigest(), 'biomes': biomes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--resources', type=Path, default=lab.RESOURCES, help='prepared resource directory')
    parser.add_argument('--binary', type=Path, help='already-built world_probe executable')
    parser.add_argument('--record', action='store_true', help='print recorded digests instead of comparing')
    args = parser.parse_args()
    resources = args.resources.resolve()
    if not (resources / 'worldgen/world_presets.json').is_file():
        if args.resources != lab.RESOURCES or args.record:
            parser.error(f'prepared worldgen data not found: {resources}')
        print(f'SKIP chunk preset corpus: prepared worldgen data not found at {resources}')
        return
    binary = args.binary.resolve() if args.binary else rust_binary()
    observed = {}
    for preset, seed, packs in cases():
        result = dump(binary, resources, preset, seed, packs)
        observed[f'{preset}/{seed}/{packs}'] = digest(result)
        if (preset, seed, packs) == ('minecraft:normal', SEEDS[0], 'vanilla'):
            if result != dump(binary, resources, None, seed, packs, dimension='overworld'):
                raise SystemExit('the implicit normal preset differs from explicit minecraft:normal')
    for seed in SEEDS:
        def result(preset, packs):
            return observed[f'{preset}/{seed}/{packs}']

        normal = result('minecraft:normal', 'vanilla')
        if normal == result('minecraft:amplified', 'vanilla'):
            raise SystemExit(f'amplified did not change terrain for seed {seed}')
        if normal == result('minecraft:normal', 'overlay'):
            raise SystemExit(f'data packs did not change terrain for seed {seed}')
        if result('minecraft:single_biome_surface', 'vanilla')['biomes'] != ['minecraft:plains']:
            raise SystemExit(f'single_biome_surface did not select plains for seed {seed}')
        if result('horizon_test:fixed_desert', 'custom')['biomes'] != ['minecraft:desert']:
            raise SystemExit(f'custom preset did not select desert for seed {seed}')
    if args.record:
        print(json.dumps(observed, indent=2, sort_keys=True))
        return
    expected = json.loads(CORPUS.read_text())
    mismatches = [key for key in sorted(expected.keys() | observed.keys()) if expected.get(key) != observed.get(key)]
    for key in mismatches:
        print(f'{key}: expected {expected.get(key)}, got {observed.get(key)}')
    if mismatches:
        raise SystemExit(f'{len(mismatches)} changed chunk preset digests')
    print(f'{len(observed)}/{len(expected)} chunk preset digests match; implicit normal is byte-identical')


if __name__ == '__main__':
    main()
