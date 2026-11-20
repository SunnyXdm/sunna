"""Contact sheets for the update proof; every captured state, both sizes."""
from pathlib import Path
import subprocess
out=Path(__file__).resolve().parent/'design/shots-app'
def sheet(name,files,columns=3):
 if not files:return
 args=['montage','-font','DejaVu-Sans','-pointsize','12','-background','#19191c','-fill','#eeeeef']
 for path in files:args+=['-label',path.stem.replace('updates-',''),str(path)]
 subprocess.run(args+['-thumbnail','360x250','-geometry','+6+8','-tile',f'{columns}x',str(out/name)],check=True)
for size in ['default','minimum']:
 for kind in ['host','app','confirm']:
  sheet(f'updates-contact-{kind}-{size}.png',sorted(out.glob(f'updates-{kind}-*-{size}.png')))
sheet('updates-contact-webkit.png',sorted(out.glob('updates-webkit-*.png')),2)
sheet('updates-contact-flow.png',[out/name for name in ['updates-toolbar.png','updates-flow-updating.png','updates-flow-done.png','updates-request-failed.png']],2)
sheet('updates-contact-real-host.png',[out/f'updates-native-toolbar-{state}.png' for state in ['menu','confirm','requested'] if (out/f'updates-native-toolbar-{state}.png').exists()],3)
sheet('updates-contact-policy.png',sorted(out.glob('updates-policy-*.png')),2)
