import pathlib
import re

audit = pathlib.Path("/tmp/jackin-a15-e2e-t01/.audit")
path = audit / "a15-final-review-results.tsv"
lines = path.read_text().splitlines()
header = lines[0].split("\t")
observed = [line.split("\t") for line in lines[1:] if line.strip()]
parsed_hosts = (audit / "a15-final-review-parsed-hosts.tsv").read_text().strip().split("\t")
direct_model = next(row for row in observed if row[0] == "IT5.model" and row[6] == "restored-clean")
rows = observed[:49] + [direct_model] + observed[50:65] + [parsed_hosts] + observed[-4:]
prior = (audit / "a15-final-review-prior-results.tsv").read_text().splitlines()
expected = [line.split("\t")[0] for line in prior if line.strip()]
if len(rows) != 70 or [row[0] for row in rows] != expected:
    raise SystemExit("the canonical 70-row identity sequence does not match the approved matrix")
for row in rows:
    for index, kind in ((4, "red"), (5, "green")):
        log = pathlib.Path(row[7] if kind == "red" else row[8]).read_text().splitlines()
        passes = len({line.strip() for line in log if line.lstrip().startswith("PASS [")})
        fails = len({line.strip() for line in log if line.lstrip().startswith("FAIL [")})
        row[index] = re.sub(r"pass=\d+ fail=\d+$", f"pass={passes} fail={fails}", row[index])
    if not row[4].startswith("red=RED ") or not row[4].endswith("pass=0 fail=1"):
        raise SystemExit(f"invalid red row: {row[0]}: {row[4]}")
    if not row[5].startswith("green=GREEN ") or not row[5].endswith("pass=1 fail=0"):
        raise SystemExit(f"invalid green row: {row[0]}: {row[5]}")
    if row[0] == "IT5.model" or row[0].startswith("IT5.survivor."):
        if row[6] != "restored-clean":
            raise SystemExit(f"identity row was not restored before green: {row[0]}")
path.write_text("\n".join(["\t".join(header), *("\t".join(row) for row in rows)]) + "\n")
print("70 RED rows; 70 GREEN rows; 5 direct identity rows green after restore")
