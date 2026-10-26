# scan_fonts.py
import re, struct, sys
d = open(sys.argv[1], 'rb').read()
total = 0
for m in re.finditer(rb'OTTO|\x00\x01\x00\x00', d):
    o = m.start()
    n = struct.unpack('>H', d[o+4:o+6])[0]
    if not 4 <= n <= 60:
        continue
    recs = [d[o+12+16*i : o+28+16*i] for i in range(n)]
    if len(recs) != n or not all(re.fullmatch(rb'[ -~]{4}', r[:4]) for r in recs):
        continue
    end = max(struct.unpack('>I', r[8:12])[0] + struct.unpack('>I', r[12:16])[0] for r in recs)
    if o + end > len(d):
        continue
    total += end
    print(f"{o:#012x}  {end/1024:10.0f} KiB  tables={n}")
print(f"TOTAL fonts: {total/1048576:.1f} MiB")
