import {execFileSync} from 'node:child_process';
import {readdirSync} from 'node:fs';
const dir=new URL('../shots/motions/',import.meta.url).pathname;
for(const file of readdirSync(dir).filter(x=>x.endsWith('.mp4'))){const phone=file.includes('phone'),width=phone?280:600,duration=+execFileSync('ffprobe',['-v','error','-show_entries','format=duration','-of','default=noprint_wrappers=1:nokey=1',dir+file],{encoding:'utf8'});execFileSync('ffmpeg',['-loglevel','error','-y','-i',dir+file,'-vf',`fps=${6/duration},scale=${width}:-1,drawtext=text='%{pts\\:hms}':x=8:y=8:fontsize=14:fontcolor=white:box=1:boxcolor=black@0.8,tile=3x2:padding=8:margin=8:color=0x303034`,'-frames:v','1',dir+file.replace('.mp4','-frames.png')]);}

for(const file of readdirSync(dir).filter(x=>x.startsWith('round2-')&&x.endsWith('.mp4'))){execFileSync('ffmpeg',['-loglevel','error','-y','-i',dir+file,'-vf',"select='lt(n,6)',scale=450:-1,drawtext=text='frame %{n}':x=8:y=8:fontsize=14:fontcolor=white:box=1:boxcolor=black@0.8,tile=3x2:padding=8:margin=8:color=0x303034",'-frames:v','1',dir+file.replace('.mp4','-first-six.png')]);}
