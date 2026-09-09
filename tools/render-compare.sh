#!/usr/bin/env bash
set -euo pipefail
comparison_legacy=$(realpath "${1:?pass the capytest source directory}")
comparison_output=$(realpath -m "${2:-target/render-comparison}")
comparison_tools=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$comparison_output"
if ! rg -q '^render-parity = ' "$comparison_legacy/Cargo.toml"; then
    git -C "$comparison_legacy" apply --check "$comparison_tools/legacy-parity.patch"
    git -C "$comparison_legacy" apply "$comparison_tools/legacy-parity.patch"
fi
cargo build --release --manifest-path "$comparison_legacy/Cargo.toml" --no-default-features --features render-parity --bin bench
comparison_target=$(cargo metadata --manifest-path "$comparison_legacy/Cargo.toml" --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cp "$comparison_target/release/bench" "$comparison_output/legacy"
cargo build --release -p capy-client --features render-bench --bin render-bench
comparison_target=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
cp "$comparison_target/release/render-bench" "$comparison_output/candidate"
python3 - "$comparison_output" <<'PY'
import array, csv, hashlib, json, math, os, pathlib, statistics, struct, subprocess, sys
root = pathlib.Path(sys.argv[1])
width, height, warmup, frames, repeats = 2940, 1846, 200, 600, 3
env = dict(os.environ, WGPU_BACKEND='vulkan')
scenarios = {'exterior': [], 'interior': ['--inside'], 'rotated': ['--moving', '--pose-frame', '120'], 'aerial': ['--orbit', '--pose-frame', '450']}

def run_new(directory, name, options, prepare=False):
    command = [str(root / 'candidate'), '--opaque-parity', '--blocks', '9', '--width', str(width), '--height', str(height), '--warmup', str(warmup), '--frames', str(frames), '--capture', str(directory / f'{name}.rgba'), '--output', str(directory / f'{name}.json')] + options
    if prepare:
        command += ['--legacy-snapshot', str(directory / 'scene.bin')]
    subprocess.run(command, env=env, check=True)

def stats(values):
    values = sorted(values)
    return {'median': statistics.median(values), 'p95': values[math.ceil(len(values) * .95) - 1], 'p99': values[math.ceil(len(values) * .99) - 1], 'worst': values[-1]}

def inverse(matrix):
    rows = [[matrix[c * 4 + r] for c in range(4)] + [float(r == c) for c in range(4)] for r in range(4)]
    for column in range(4):
        pivot = max(range(column, 4), key=lambda row: abs(rows[row][column]))
        rows[column], rows[pivot] = rows[pivot], rows[column]
        divisor = rows[column][column]
        rows[column] = [v / divisor for v in rows[column]]
        for row in range(4):
            if row != column:
                factor = rows[row][column]
                rows[row] = [a - factor * b for a, b in zip(rows[row], rows[column])]
    return [rows[r][c + 4] for c in range(4) for r in range(4)]

def project(matrix, point):
    v = [sum(matrix[c * 4 + r] * point[c] for c in range(3)) + matrix[12 + r] for r in range(4)]
    return [v[i] / v[3] for i in range(3)]

def rotate(point, quaternion):
    x, y, z, w = quaternion
    a, b, c = point
    t = [2 * (y * c - z * b), 2 * (z * a - x * c), 2 * (x * b - y * a)]
    return [a + w * t[0] + y * t[2] - z * t[1], b + w * t[1] + z * t[0] - x * t[2], c + w * t[2] + x * t[1] - y * t[0]]

def oracle_scene(directory):
    data = (directory / 'scene.bin').read_bytes()
    assert data[:10] == b'CAPYBENCH\x02'
    offset = 10 + 41 * 4
    count, = struct.unpack_from('<I', data, offset)
    offset += 4
    models, decoded = [], {}
    for _ in range(count):
        _, _, pitch, sx, sy, sz = struct.unpack_from('<IBf3i', data, offset)
        offset += 21
        transform = struct.unpack_from('<7f', data, offset)
        offset += 28 + 20
        length, = struct.unpack_from('<I', data, offset)
        offset += 4
        encoded = data[offset:offset + length]
        offset += length
        if encoded not in decoded:
            voxels = bytearray()
            for material, run in struct.iter_unpack('<BH', encoded):
                voxels.extend(bytes([material]) * run)
            assert len(voxels) == sx * sy * sz
            decoded[encoded] = voxels
        models.append(((sx, sy, sz), pitch, transform[:3], tuple(-v for v in transform[3:6]) + (transform[6],), decoded[encoded]))
    assert offset == len(data)
    matrix = struct.unpack('<19f', (directory / 'preparation.camera').read_bytes())[:16]
    fov, = struct.unpack_from("<f", data, 10 + 6 * 4)
    return models, matrix, inverse(matrix), 2 * math.tan(fov / 2) / height / 128 * math.sqrt(2)

def oracle_ray(scene, x, y):
    models, matrix, inv, pixel_angle = scene
    start = project(inv, [(x + .5) * 2 / width - 1, 1 - (y + .5) * 2 / height, 0])
    end = project(inv, [(x + .5) * 2 / width - 1, 1 - (y + .5) * 2 / height, 1])
    direction = [b - a for a, b in zip(start, end)]
    candidates = []
    for size, pitch, translation, rotation, voxels in models:
        origin = [v / pitch for v in rotate([a - b for a, b in zip(start, translation)], rotation)]
        delta = [v / pitch for v in rotate(direction, rotation)]
        near, far = 0.0, 1.0
        for axis in range(3):
            if abs(delta[axis]) < 1e-14:
                if not 0 <= origin[axis] < size[axis]:
                    far = -1.0
            else:
                a, b = -origin[axis] / delta[axis], (size[axis] - origin[axis]) / delta[axis]
                near, far = max(near, min(a, b)), min(far, max(a, b))
        if near < far:
            candidates.append((near, far, size, origin, delta, voxels))
    nearest = 1.0
    hits = []
    for near, far, size, origin, delta, voxels in sorted(candidates, key=lambda candidate: candidate[0]):
        cell = [min(size[i] - 1, max(0, math.floor(origin[i] + delta[i] * near + math.copysign(1e-8, delta[i])))) for i in range(3)]
        step = [1 if v > 0 else -1 for v in delta]
        increments = [abs(1 / v) if abs(v) > 1e-14 else math.inf for v in delta]
        crossing = [((cell[i] + (step[i] > 0)) - origin[i]) / delta[i] if abs(delta[i]) > 1e-14 else math.inf for i in range(3)]
        t = near
        occupied = False
        while t <= far and all(0 <= cell[i] < size[i] for i in range(3)):
            value = voxels[cell[0] + size[0] * (cell[1] + size[1] * cell[2])]
            if value and not occupied:
                point = [origin[i] + delta[i] * t for i in range(3)]
                tolerance = math.sqrt(sum(v * v for v in delta)) * t * pixel_angle + 1e-4
                axis = min(range(3), key=lambda i: abs(point[i] - round(point[i])))
                cosine = abs(delta[axis]) / math.sqrt(sum(v * v for v in delta))
                tolerance /= max(cosine, 1e-6)
                boundary = sum(abs(v - round(v)) <= tolerance for v in point) > 1
                hits.append((t, value))
                if boundary:
                    choices = [[cell[i]] if i == axis or abs(point[i] - round(point[i])) > tolerance else [round(point[i]) - 1, round(point[i])] for i in range(3)]
                    for xx in choices[0]:
                        for yy in choices[1]:
                            for zz in choices[2]:
                                adjacent = [xx, yy, zz]
                                if not all(0 <= adjacent[i] < size[i] for i in range(3)):
                                    continue
                                colour = voxels[xx + size[0] * (yy + size[1] * zz)]
                                adjacent[axis] -= step[axis]
                                exposed = not all(0 <= adjacent[i] < size[i] for i in range(3)) or not voxels[adjacent[0] + size[0] * (adjacent[1] + size[1] * adjacent[2])]
                                if colour and exposed:
                                    hits.append((t, colour))
                if not boundary:
                    nearest = min(nearest, t)
                    break
            occupied = bool(value)
            axis = min(range(3), key=lambda i: crossing[i])
            t = crossing[axis]
            cell[axis] += step[axis]
            crossing[axis] += increments[axis]
    colors = {0: '000000ff', 8: 'd0d4d8ff', 3: 'bd8973ff', 12: 'c4bfb5ff', 20: '76797eff'}
    result = []
    nearest_depth = project(matrix, [a + b * nearest for a, b in zip(start, direction)])[2]
    for t, material in hits:
        hit_depth = project(matrix, [a + b * t for a, b in zip(start, direction)])[2]
        if hit_depth <= nearest_depth + 2e-6:
            color = int.from_bytes(bytes.fromhex(colors[material]), sys.byteorder)
            depth = project(matrix, [a + b * t for a, b in zip(start, direction)])[2]
            result.append((color, depth))
    if nearest == 1.0:
        result.append((int.from_bytes(bytes.fromhex(colors[0]), sys.byteorder), 1.0))
    return result


def parity(directory, a, b):
    ac = memoryview((directory / f'{a}.rgba').read_bytes()).cast('I')
    bc = memoryview((directory / f'{b}.rgba').read_bytes()).cast('I')
    az = array.array('f', (directory / f'{a}.depth').read_bytes())
    bz = array.array('f', (directory / f'{b}.depth').read_bytes())
    assert len(ac) == len(bc) == len(az) == len(bz) == width * height
    valid = {int.from_bytes(bytes.fromhex(color), sys.byteorder) for color in ['000000ff', 'd0d4d8ff', 'bd8973ff', 'c4bfb5ff', '76797eff']}
    color_differences, depth_differences, failures = 0, 0, []
    scene = None
    oracle_matches = 0
    checkpoints = {(2 * y + 1) * height // 24 * width + (2 * x + 1) * width // 32 for y in range(12) for x in range(16)}
    for i, (ca, cb, za, zb) in enumerate(zip(ac, bc, az, bz)):
        color_differences += ca != cb
        depth_differences += abs(za - zb) > 2e-6
        if i not in checkpoints and ca == cb and ca in valid and 0 <= za <= 1 and 0 <= zb <= 1 and abs(za - zb) <= 2e-6:
            continue
        x, y = i % width, i // width
        if scene is None:
            scene = oracle_scene(directory)
        samples = [sample for dx in [-1/128, 0, 1/128] for dy in [-1/128, 0, 1/128] for sample in oracle_ray(scene, x + dx, y + dy)]
        matching = all(any(color == c and abs(depth - z) <= 2e-6 for c, z in samples) for color, depth in [(ca, za), (cb, zb)])
        oracle_matches += matching
        if ca not in valid or cb not in valid or not math.isfinite(za + zb) or not matching:
            failures.append({'pixel': [x, y], 'colors': [hex(ca), hex(cb)], 'depths': [za, zb]})
    return {'passed': not failures, 'color_differences': color_differences, 'depth_differences': depth_differences, 'unmatched': len(failures), 'examples': failures[:40], 'depth_tolerance': 2e-6, 'ray_tolerance_pixels': 1/128, 'oracle_matches': oracle_matches, 'source_checkpoints': len(checkpoints)}

report = {'profile': 'opaque-material-v1', 'parity_policy': 'Exact material colors and depth within 2e-6; differing pixels must match source-voxel ray hits within a 1/128-pixel footprint, including ambiguous voxel edges and coplanar depth ties. No percentage-based mismatch allowance.', 'legacy_adjustments': 'Shading and postprocessing disabled; shared camera matrix; cached surfaces also used at the near plane; empty material fragments discarded.', 'scope': '81 buildings and 81 ground tiles; opaque material and depth; native resolution; offscreen GPU-completion waits; fixed poses; no physics, shadows, AO, material noise, fog, glass, upscaling or presentation', 'width': width, 'height': height, 'warmup': warmup, 'frames': frames, 'repeats': repeats, 'scenarios': {}, 'sha256': {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in ['candidate', 'legacy']}}
for scenario, options in scenarios.items():
    print('Comparing', scenario, flush=True)
    directory = root / scenario
    directory.mkdir(exist_ok=True)
    run_new(directory, 'preparation', options, True)
    settings = dict(env, CAPY_BENCH_SCENE=str(directory / 'scene.bin'), BENCH_PARITY_CAMERA=str(directory / 'preparation.camera'), BENCH_WIDTH=str(width), BENCH_HEIGHT=str(height), CAPY_WORLD_BACKEND='legacy', CAPY_RENDER_BACKEND='body', CAPY_DYNAMIC_COVERAGE='legacy', CAPY_RENDER_SCALE='1', CAPY_GPU_PROFILE='1', CAPY_GPU_TIMINGS='0', BENCH_WARMUP_FRAMES=str(warmup), BENCH_FRAMES=str(frames), BENCH_SCENARIO='idle')
    runs = []
    for run in range(repeats):
        for backend in (['legacy', 'candidate'] if run % 2 == 0 else ['candidate', 'legacy']):
            name = f'{backend}-{run}'
            if backend == 'candidate':
                run_new(directory, name, options)
            else:
                legacy_env = dict(settings, BENCH_GPU_CSV=str(directory / f'{name}-gpu.csv'), BENCH_FRAME_CSV=str(directory / f'{name}-cpu.csv'), BENCH_CAPTURE=str(directory / f'{name}.rgba'))
                with (directory / f'{name}.txt').open('wb') as output:
                    subprocess.run([str(root / 'legacy')], env=legacy_env, stdout=output, stderr=subprocess.STDOUT, check=True)
                assert 'BENCH_PROFILE opaque-material-v1' in (directory / f'{name}.txt').read_text()
        output = json.loads((directory / f'candidate-{run}.json').read_text())
        candidate = output['report']
        assert output['opaque_parity'] and len(candidate['samples']) == frames
        assert (candidate['width'], candidate['height'], candidate['warmup']) == (width, height, warmup)
        assert (directory / f'candidate-{run}.camera').read_bytes() == (directory / 'preparation.camera').read_bytes()
        cpu = list(csv.DictReader((directory / f'legacy-{run}-cpu.csv').open()))
        gpu = [float(row['ms']) for row in csv.DictReader((directory / f'legacy-{run}-gpu.csv').open()) if row['stage'] == 'frame']
        assert len(cpu) == len(gpu) == frames
        entry = {'candidate': {key: stats([sample[key] for sample in candidate['samples']]) for key in ['wall_ms', 'gpu_ms']}, 'legacy': {'wall_ms': stats([float(row['wall_ms']) for row in cpu]), 'gpu_ms': stats(gpu)}, 'geometry_bytes': candidate['geometry_bytes'], 'parity': parity(directory, f'candidate-{run}', f'legacy-{run}')}
        runs.append(entry)
    result = {'runs': runs, 'sha256': {name: hashlib.sha256((directory / name).read_bytes()).hexdigest() for name in ['scene.bin', 'preparation.camera']}}
    result['timings'] = {backend: {metric: {stat: statistics.median(run[backend][metric][stat] for run in runs) for stat in ['median', 'p95', 'p99']} for metric in ['wall_ms', 'gpu_ms']} for backend in ['candidate', 'legacy']}
    result['performance_passed'] = all(result['timings']['candidate'][metric][stat] <= result['timings']['legacy'][metric][stat] for metric in ['wall_ms', 'gpu_ms'] for stat in ['median', 'p95'])
    result['parity_passed'] = all(run['parity']['passed'] for run in runs)
    report['scenarios'][scenario] = result
    (root / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
    print(scenario, json.dumps(result['timings']), 'parity', result['parity_passed'], 'performance', result['performance_passed'], flush=True)
report['passed'] = all(s['performance_passed'] and s['parity_passed'] for s in report['scenarios'].values())
(root / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
print('PASS' if report['passed'] else 'FAIL', root / 'comparison.json')
sys.exit(0 if report['passed'] else 1)
PY
