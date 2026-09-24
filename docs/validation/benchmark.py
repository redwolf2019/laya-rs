"""Bounded Linux Docker benchmark. Run from repository root; Python stdlib only."""
import argparse
import collections
import contextlib
import http.client
import concurrent.futures
import copy
import hashlib
import json
import math
import pathlib
import runpy
import socket
import sys
import threading
import time
import urllib.error

smoke = runpy.run_path(str(pathlib.Path(__file__).with_name('docker-smoke.py')))
docker, request = smoke['docker'], smoke['request']


def summarize(rows):
    successes = [r for r in rows if r['outcome'] == 'success']
    elapsed = max((r['end_s'] for r in rows), default=0)
    completed = sum(r['status'] != 0 for r in rows)
    eligible = elapsed >= 60 and completed >= 100
    times = sorted(r['latency_ms'] for r in successes)
    percentiles = {f'p{p}': times[math.ceil(len(times) * p / 100) - 1]
                   for p in (50, 95, 99)} if eligible and times else None
    count = len(rows)
    outcomes = collections.Counter(r['outcome'] for r in rows)
    return dict(attempts=count, completed=completed, successes=len(successes),
                elapsed_s=elapsed, protocol_met=eligible, latency_ms=percentiles,
                success_qps=len(successes) / elapsed if eligible else None,
                error_rate=(count - len(successes)) / count if count else None,
                timeout_rate=sum(outcomes[k] for k in
                    ('queue_timeout', 'inference_timeout', 'transport_timeout')) / count if count else None,
                rejection_rate=sum(r['status'] in (429, 503) for r in rows) / count if count else None,
                outcomes=dict(outcomes))


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def invoke(base, fixture, epoch):
    start = time.monotonic()
    status, outcome = 0, 'transport_error'
    try:
        status, body = request(base, '/v1/system-one', fixture['request_json'])
        value = json.loads(body)
        if status == 200:
            smoke['check_answer'](value, copy.deepcopy(fixture['response']))
            outcome = 'success'
        else:
            outcome = value['error']['code']
    except (TimeoutError, socket.timeout):
        outcome = 'transport_timeout'
    except urllib.error.URLError as error:
        outcome = 'transport_timeout' if isinstance(error.reason, TimeoutError) else 'transport_error'
    except (ConnectionError, OSError, http.client.HTTPException):
        outcome = 'transport_error'
    except (AssertionError, KeyError, TypeError, ValueError):
        outcome = 'answer_mismatch'
    end = time.monotonic()
    return dict(start_s=start - epoch, end_s=end - epoch,
                latency_ms=(end - start) * 1000, status=status, outcome=outcome)


def observe(cid, base, epoch):
    # proc stat is PID 1 only: sampling exec processes do not count as service CPU.
    raw = docker('exec', cid, 'sh', '-ec',
                 'cat /proc/1/stat; cat /proc/1/status; cat /sys/fs/cgroup/memory.events',
                 timeout=10)
    lines = raw.splitlines()
    fields = lines[0].rsplit(')', 1)[1].split()
    rss = next(int(line.split()[1]) for line in lines if line.startswith('VmRSS:'))
    status, metrics = request(base, '/metrics')
    if status != 200:
        raise RuntimeError('metrics unavailable')
    gauges = dict(line.split() for line in metrics.splitlines()
                  if line.startswith(('laya_queue_size ', 'laya_inference_inflight ')))
    return dict(time_s=time.monotonic() - epoch, cpu_ticks=int(fields[11]) + int(fields[12]),
                rss_kib=rss, queue=int(gauges['laya_queue_size']),
                inflight=int(gauges['laya_inference_inflight']), raw=raw, metrics=metrics)


def sample_resources(cid, base, epoch, stop, path):
    rows = []
    with path.open('x') as output:
        while True:
            row = observe(cid, base, epoch)
            rows.append(row)
            output.write(json.dumps(row) + '\n')
            output.flush()
            if stop.wait(1):
                break
    return rows


def load(base, fixture, clients, epoch, path):
    lock, rows = threading.Lock(), []
    counters = {'started': 0, 'completed': 0}
    with path.open('x') as output:
        def worker():
            while True:
                with lock:
                    elapsed = time.monotonic() - epoch
                    if (elapsed >= 60 and counters['completed'] >= 100) or elapsed >= 300:
                        return
                    if counters['started'] >= 10000:
                        return
                    counters['started'] += 1
                row = invoke(base, fixture, epoch)
                with lock:
                    rows.append(row)
                    counters['completed'] += row['status'] != 0
                    output.write(json.dumps(row) + '\n')
                    output.flush()
        with concurrent.futures.ThreadPoolExecutor(max_workers=clients) as pool:
            futures = [pool.submit(worker) for _ in range(clients)]
            for future in futures:
                future.result()
    return rows


def group(cid, base, fixture, clients, directory):
    directory.mkdir()
    warmup, epoch = [], time.monotonic()
    while len(warmup) < 10 or time.monotonic() - epoch < 10:
        row = invoke(base, fixture, epoch)
        warmup.append(row)
        if row['outcome'] != 'success':
            break
    write_json(directory / 'warmup.json', warmup)
    if any(row['outcome'] != 'success' for row in warmup):
        raise RuntimeError('warmup failed; samples retained')
    epoch, stop = time.monotonic(), threading.Event()
    before = observe(cid, base, epoch)
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        sampler = pool.submit(sample_resources, cid, base, epoch, stop, directory / 'resources.jsonl')
        try:
            rows = load(base, fixture, clients, epoch, directory / 'requests.jsonl')
        finally:
            stop.set()
        resources = sampler.result()
    after = observe(cid, base, epoch)
    write_json(directory / 'boundaries.json', dict(before=before, after=after))
    report = summarize(rows)
    ticks = int(docker('exec', cid, 'getconf', 'CLK_TCK'))
    report.update(clients=clients, cpu_percent=100 * (after['cpu_ticks'] - before['cpu_ticks']) /
                  ticks / (after['time_s'] - before['time_s']), clock_ticks=ticks,
                  rss_peak_kib=max(r['rss_kib'] for r in resources + [before, after]),
                  queue_max=max(r['queue'] for r in resources),
                  inflight_max=max(r['inflight'] for r in resources), resource_samples=len(resources))
    write_json(directory / 'summary.json', report)
    print(directory, json.dumps(report), flush=True)
    return report


def inventory(cid, directory):
    write_json(directory / 'container.json', smoke['inspect'](cid))
    command = ('uname -a; cat /proc/cpuinfo; cat /sys/fs/cgroup/cpu.max; '
               'cat /sys/fs/cgroup/cpuset.cpus.effective; cat /sys/fs/cgroup/memory.max; '
               'cat /sys/fs/cgroup/memory.swap.max; cat /sys/fs/cgroup/memory.events; '
               'sha256sum /usr/local/bin/laya-server /opt/onnxruntime/lib/libonnxruntime.so; '
               'dpkg-query -W')
    (directory / 'environment.log').write_text(docker('exec', cid, 'sh', '-ec', command) + '\n')


def configuration(args, threads, slots, fixture):
    directory = args.output / f't{threads}-s{slots}'
    directory.mkdir()
    started = time.monotonic()
    with smoke['container'](args.image, args.bundle, '--threads', str(threads),
            '--max-concurrency', str(slots), '--inter-op-threads', '1',
            '--queue-capacity', '32', '--queue-timeout', '30',
            '--inference-timeout', '120', '--shutdown-grace', '120') as cid:
        try:
            base = smoke['base_url'](cid)
            cold = dict(create_to_ready_s=time.monotonic() - started)
            cold['first_request'] = invoke(base, fixture, time.monotonic())
            write_json(directory / 'cold.json', cold)
            if cold['first_request']['outcome'] != 'success':
                raise RuntimeError('first request failed')
            inventory(cid, directory)
            reports = [group(cid, base, fixture, c, directory / f'c{c}') for c in (1, 2, 4, 8)]
            docker('stop', '--timeout', '130', cid)
        finally:
            write_json(directory / 'exit.json', smoke['inspect'](cid)['State'])
        smoke['exit_code'](cid, 0)
    return all(r['protocol_met'] and r['error_rate'] == 0 for r in reports)


def record_failure(directory, error):
    reports = {}
    for path in directory.glob('c*/requests.jsonl'):
        rows = [json.loads(line) for line in path.read_text().splitlines()]
        summary = path.with_name('summary.json')
        if summary.exists():
            report = json.loads(summary.read_text())
        else:
            report = summarize(rows)
            report['configuration_failed'] = True
            write_json(summary, report)
        reports[path.parent.name] = dict(attempts=report['attempts'], completed=report['completed'])
    write_json(directory / 'failure.json', dict(type=type(error).__name__,
               detail=str(error), measured_groups=reports,
               measured_completed=sum(r['completed'] for r in reports.values())))


def run(args):
    args.bundle = args.bundle.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    fixture_path = pathlib.Path('tests/fixtures/system-one/mixed-6.json')
    fixture = json.loads(fixture_path.read_text())
    write_json(args.output / 'protocol.json', dict(fixture=str(fixture_path),
        fixture_sha256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
        request_json=fixture['request_json'], usage=fixture['response']['usage'],
        configurations=[[1, 1], [2, 1], [2, 2]], clients=[1, 2, 4, 8],
        warmup='10 seconds AND 10 sequential successes per group',
        measurement='60 seconds AND 100 completed HTTP requests; cap 300 seconds/10000 attempts',
        client_timeout_s=150, quantiles='nearest rank, successful requests only',
        qps='successful requests / last completion offset, including drain',
        python=sys.version))
    success = True
    for threads, slots in ((1, 1), (2, 1), (2, 2)):
        try:
            with (args.output / f't{threads}-s{slots}.log').open('x') as output:
                with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
                    success = configuration(args, threads, slots, fixture) and success
            print('completed configuration', threads, slots, flush=True)
        except Exception as error:
            record_failure(args.output / f't{threads}-s{slots}', error)
            success = False
            print('FAILED configuration', threads, slots, repr(error), flush=True)
    return 0 if success else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    summary = commands.add_parser('summarize')
    summary.add_argument('samples', type=pathlib.Path)
    bench = commands.add_parser('run')
    bench.add_argument('image')
    bench.add_argument('bundle', type=pathlib.Path)
    bench.add_argument('output', type=pathlib.Path)
    args = parser.parse_args()
    if args.command == 'run':
        return run(args)
    rows = [json.loads(line) for line in args.samples.read_text().splitlines()]
    print(json.dumps(summarize(rows), indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())
