import importlib.util
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("candidate", root / "calculator.py")
if spec is None or spec.loader is None:
    raise RuntimeError("Cannot load candidate module")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
for a, b, expected in [(2, 3, 5), (-2, -3, -5)]:
    actual = module.add(a, b)
    if actual != expected:
        raise AssertionError(f"add({a}, {b}) returned {actual}; expected {expected}")
print("PASS: required arithmetic cases")
