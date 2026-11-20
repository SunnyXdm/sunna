// The real Settings sheet with sharing being turned on: one continuous crop to
// the sheet itself (500 x 612 at 1x), nothing joined. Needs fixture.py serving on 127.0.0.1:8795.
import {launch,work} from './browser.mjs';
import {execFileSync} from 'node:child_process';

const {page,close}=await launch({port:8798});
const out='docs/images/recordings/';
const CROP='1000x1224+480+48';
try{
  await page.goto('http://127.0.0.1:8795/ui/?count=6');
  await page.sleep(800);
  await page.click('#settings-button');
  await page.sleep(900);
  await page.shot(work+'/sheet-off.png');
  await page.record(work+'/switch.mp4',async()=>{
    await page.sleep(900);
    await page.click('#share-host');
    await page.sleep(3600);
  },{scale:2});
  await page.shot(work+'/sheet-on.png');
  execFileSync('magick',[work+'/sheet-on.png','-crop',CROP,'+repage','-quality','93',out+'sharing-poster.webp']);
  execFileSync('magick',[work+'/sheet-off.png','-crop',CROP,'+repage','-quality','93',out+'switch-off.webp']);
  execFileSync('magick',[work+'/sheet-on.png','-crop',CROP,'+repage','-quality','93',out+'switch-on.webp']);
  execFileSync('ffmpeg',['-y','-loglevel','error','-i',work+'/switch.mp4','-vf','fps=24,scale=1960:1320:flags=lanczos,crop=1000:1224:480:48,setsar=1',
    '-pix_fmt','yuv420p','-c:v','libx264','-crf','24','-preset','slow','-movflags','+faststart','-an',out+'sharing.mp4']);
}finally{await close()}
