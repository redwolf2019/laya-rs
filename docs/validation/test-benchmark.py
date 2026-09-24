"""CLI statistics contract; no Docker or model required."""
import json
import http.server
import runpy
import threading
import time
import pathlib
import subprocess
import sys
import tempfile

script = pathlib.Path(__file__).with_name('benchmark.py')
with tempfile.TemporaryDirectory() as directory:
    root = pathlib.Path(directory)
    samples = [dict(start_s=i, end_s=i + 0.1, latency_ms=i + 1,
                    status=200, outcome='success') for i in range(100)]
    samples += [dict(start_s=100, end_s=101, latency_ms=1000, status=status,
                     outcome=outcome) for status, outcome in
                [(429, 'queue_full'), (503, 'queue_timeout'),
                 (504, 'inference_timeout'), (0, 'transport_timeout'),
                 (500, 'inference_failed'), (200, 'answer_mismatch')]]
    path = root / 'requests.jsonl'
    path.write_text(''.join(json.dumps(row) + '\n' for row in samples))
    result = subprocess.run([sys.executable, str(script), 'summarize', str(path)],
                            capture_output=True, text=True, check=True)
    report = json.loads(result.stdout)
    assert report['completed'] == 105 and report['attempts'] == 106
    assert report['successes'] == 100 and report['protocol_met']
    assert report['latency_ms'] == {'p50': 50, 'p95': 95, 'p99': 99}
    assert report['success_qps'] == 100 / 101
    assert report['timeout_rate'] == 3 / 106
    assert report['rejection_rate'] == 2 / 106
    assert report['error_rate'] == 6 / 106
    path.write_text(json.dumps(samples[0]) + '\n')
    report = json.loads(subprocess.check_output(
        [sys.executable, str(script), 'summarize', str(path)], text=True))
    assert not report['protocol_met']
    assert report['latency_ms'] is None and report['success_qps'] is None
    path.write_text('')
    report = json.loads(subprocess.check_output(
        [sys.executable, str(script), 'summarize', str(path)], text=True))
    assert report['attempts'] == 0 and not report['protocol_met']
print('PASS: CLI percentiles, success QPS, failures, insufficient and empty samples')


class TruncatedResponse(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        self.send_response(200)
        self.send_header('Content-Length', '100')
        self.end_headers()
        self.wfile.write(b'{')
        self.close_connection = True

    def log_message(self, *_):
        pass


with http.server.HTTPServer(('127.0.0.1', 0), TruncatedResponse) as server:
    thread = threading.Thread(target=server.handle_request)
    thread.start()
    try:
        sample = runpy.run_path(str(script))['invoke'](
            f'http://127.0.0.1:{server.server_port}',
            {'request_json': '{}'}, time.monotonic())
        assert sample['status'] == 0 and sample['outcome'] == 'transport_error'
    finally:
        thread.join(timeout=5)
    assert not thread.is_alive()
print('PASS: truncated real HTTP response produces a transport failure sample')
