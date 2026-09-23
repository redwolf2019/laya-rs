"""CPython-only #11 text oracle; no model and no Rust-generated expectations.

Run in the pinned #7 Python 3.11.16 container; write a new output path.
The two render modes are the frozen official API contract (§2.3).
"""

import hashlib
import json
import math
import platform
import random
import struct
import sys
from pathlib import Path


def values():
    yield from ["null", "true", "false", "{}", "[]", "0", "-0", "1", "-1",
                "1.0", "1e0", "-0.0", "-0e999", "1E+0007", "1e-0007",
                "1e400", "-1e400", "1e-4000", "-1e-4000",
                "9007199254740993.0", "1.00000762939453125", "-1.00000762939453125",
                "2.2250738585072012e-308", "2.4703282292062327e-324",
                "2.4703282292062328e-324", "1.7976931348623158e308",
                "1.7976931348623159e308", "0." + "0" * 400 + "1e400",
                "1e" + "9" * 400, "-1e-" + "9" * 400]
    characters = ''.join(map(chr, range(128))) + "中文😄\u0080\u2028\u2029\uffff\U0010ffff"
    yield json.dumps(characters)
    yield json.dumps({characters: [characters, {"10": 1, "2": None}]})
    yield r'{"b":{"z":1,"a":2,"z":3},"a":[{"2":0,"1":1}],"b":{"中":"\ud83d\ude04","x":1,"中":"😄"}}'
    for exponent in [-324, -308, -7, -6, -5, -4, -3, 0, 15, 16, 17, 20, 21, 22, 23, 100, 308]:
        value = float(f"1e{exponent}")
        for adjacent in [math.nextafter(value, 0), value, math.nextafter(value, math.inf)]:
            yield repr(adjacent)
            yield repr(-adjacent)
    rng = random.Random(11)
    for _ in range(256):
        value = struct.unpack(">d", rng.getrandbits(64).to_bytes(8, "big"))[0]
        if math.isfinite(value):
            # Long exact decimal stresses parsing as well as shortest rendering.
            yield format(value, ".1074f") if abs(value) < 1 else format(value, ".53e")


def generate():
    assert platform.python_version() == "3.11.16"
    sys.set_int_max_str_digits(0)
    cases = []
    for raw in values():
        value = json.loads(raw)
        cases.append(dict(input_json=raw,
                          state_text=value if isinstance(value, str) else json.dumps(value, ensure_ascii=False),
                          instructions_text=value if isinstance(value, str) else json.dumps(value)))
    rejected = []
    for raw in ["01", "-01", "1e", "1e+", "1.e2", "--1", "[" * 1100 + "0" + "]" * 1100]:
        try:
            json.loads(raw)
        except (ValueError, RecursionError) as error:
            rejected.append(dict(input_json=raw, python_error=type(error).__name__))
        else:
            raise AssertionError("oracle unexpectedly accepted rejection case")
    return dict(python=platform.python_version(), int_max_str_digits=sys.get_int_max_str_digits(),
                generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                cases=cases, rejected=rejected)


if __name__ == "__main__":
    with Path(sys.argv[1]).open("x", encoding="utf-8") as output:
        json.dump(generate(), output, ensure_ascii=True, indent=2, allow_nan=False)
        output.write("\n")
