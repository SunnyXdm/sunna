"""Contact sheets for the captured mock matrix, inventory states and native app."""
from pathlib import Path
import subprocess
OUT=Path(__file__).parent/'design/shots-app'
VIEWS=['devices','wall','space','type','gallery']
def sheet(name,files,columns=5):
 files=[p for p in files if p.exists()]
 if not files:return
 args=['montage','-font','DejaVu-Sans','-pointsize','13','-background','#252529','-fill','#eeeeee']
 for path in files:args+=['-label',path.stem,str(path)]
 subprocess.run(args+['-thumbnail','320x226','-geometry','+6+8','-tile',f'{columns}x',str(OUT/name)],check=True)
for size,name in [('default','compositions'),('minimum','minimum')]:
 sheet(name+'-contact.png',[OUT/f'{view}-{n}-{size}.png' for n in [0,1,2,6,20] for view in VIEWS])
for size in ['default','minimum','retina']:
 sheet(f'webkit-{size}-contact.png',[OUT/f'webkit-{view}-{count}-{size}.png' for count in [0,1,6,20] for view in VIEWS])
sheet('retina-contact.png',[OUT/f'{view}-{count}-retina.png' for count in [0,1,6,20] for view in VIEWS])
sheet('states-contact.png',[OUT/f'{view}-state-{state}.png' for state in ['idle','checking','ready','ready-no-latency','busy','busy-with-viewer','wrong-key','update-needed','not-found','invalid','unreachable','never-connected'] for view in VIEWS])
sheet('sheets-contact.png',[OUT/f'{view}-{s}.png' for view in ['devices','space'] for s in ['view-menu','add','add-ready','tailscale','settings','settings-bottom','licenses','computer-menu','edit','remove','qr']],4)
sheet('extra-states-contact.png',[*sorted(OUT.glob('*-form-*.png')),*sorted(OUT.glob('*-tailscale-error.png')),*sorted(OUT.glob('*-tailscale-empty.png')),*sorted(OUT.glob('*-qr-error.png')),*sorted(OUT.glob('*-removed-toast.png')),*sorted(OUT.glob('access-*.png'))],4)
sheet('kinds-contact.png',[OUT/f'device-kind-{k}.png' for k in ['laptop','desktop','mini','server','vm','unknown']],3)
sheet('long-and-overflow-contact.png',[OUT/f'{view}-{s}.png' for s in ['long-default','long-minimum','20-bottom-default','20-bottom-minimum'] for view in VIEWS])
sheet('webkit-contact.png',[OUT/f'webkit-{view}{size}.png' for size in ['','-minimum'] for view in VIEWS])
sheet('webkit-session-contact.png',[OUT/f'webkit-{s}.png' for s in ['settings-space','after-drag','viewer','returned','remove-picture']],3)
for view in VIEWS:
 for action in ['connect-leave','cancel','failure']:
  movie=OUT/f'{view}-{action}.mp4'
  if movie.exists():subprocess.run(['ffmpeg','-loglevel','error','-y','-i',str(movie),'-vf','fps=3,scale=320:-1,tile=6x3','-frames:v','1',str(OUT/f'motion-{view}-{action}.png')],check=True)

sheet('review-four-views.png',[OUT/f'{view}-6-default.png' for view in VIEWS[:4]],2)
sheet('review-craft-pass.png',[OUT/f'{view}-6-default.png' for view in ['gallery','devices']],2)
movie=OUT/'webkit-connect-leave.mp4'
if movie.exists():
 subprocess.run(['ffmpeg','-loglevel','error','-y','-i',str(movie),'-vf','fps=1/5,scale=320:-1,tile=3x3','-frames:v','1',str(OUT/'motion-webkit-session.png')],check=True)
