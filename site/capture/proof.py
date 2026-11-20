"""Make the contact sheet from the delivered MP4 and native-size feature proofs."""
from pathlib import Path
import subprocess as sp

media = Path('docs/images/recordings')
shots = Path('~/ext/work/site/shots').expanduser()
shots.mkdir(exist_ok=True)
# The teal wallpaper grows with the launch screen. Locate the spring in the
# delivered encode so a variation in CDP timestamps cannot skip the transition.
raw = sp.check_output(['ffmpeg','-loglevel','error','-i',str(media/'hero.mp4'),
                       '-vf','scale=80:54','-f','rawvideo','-pix_fmt','rgb24','-'])
scores = []
for start in range(0,len(raw),80*54*3):
    data = raw[start:start+80*54*3]
    scores.append(sum(g>100 and g-r>65 and b-r>65 for r,g,b in zip(data[::3],data[1::3],data[2::3])))
peak = max(scores)
def first_fraction(fraction):
    return next(i for i,score in enumerate(scores) if score>=peak*fraction)
def last_fraction(fraction):
    return max(i for i,score in enumerate(scores) if score>=peak*fraction)
grow = [first_fraction(f) for f in (.08,.32,.65,.95)]
shrink = [last_fraction(f) for f in (.9,.5,.12)]
frames = [0, grow[0]-8, *grow, grow[-1]+30, grow[-1]+65, grow[-1]+100,
          grow[-1]+130,shrink[0]-25, *shrink,shrink[-1]+12,len(scores)-1]
assert len(set(frames))==16 and frames==sorted(frames),frames
select = '+'.join(f'eq(n\\,{n})' for n in frames)
sp.run(['ffmpeg','-y','-loglevel','error','-i',str(media/'hero.mp4'),
        '-vf',f'select={select},scale=400:-2,tile=4x4','-frames:v','1',
        str(shots/'r4-hero-sheet.png')],check=True)
print('Hero sheet frame indices (24 fps)',frames)
for path in media.glob('*.webp'):
    sp.run(['magick',str(path),str(shots/f'r4-{path.stem}.png')],check=True)
