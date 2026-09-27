import sys, struct, math
data = open(sys.argv[1], 'rb').read()
n = len(data) // 4
left = struct.unpack(f'<{n*2}h', data[:n*4])[0::2]
print(f"{n/48000:.2f} s of audio")
start = next((i for i, s in enumerate(left) if abs(s) > 100), None)
if start is None: print("all silence"); sys.exit()
print(f"sound starts at {start/48:.0f} ms")
# silent gaps (>= 5 ms of near-zero) after the start
gaps, run = [], 0
for s in left[start:]:
    if abs(s) < 30: run += 1
    else:
        if run >= 240: gaps.append(run / 48)
        run = 0
print(f"gaps of 5 ms or more: {len(gaps)}" + (f", total {sum(gaps):.0f} ms, longest {max(gaps):.0f} ms" if gaps else ""))
# pitch per 250 ms block by zero crossings
pitches = []
for b in range(start, len(left) - 12000, 12000):
    block = left[b:b + 12000]
    zc = sum(1 for a, c in zip(block, block[1:]) if (a < 0) != (c < 0))
    pitches.append(round(zc / 2 / 0.25))
print("pitch per 250 ms:", pitches[:24])
