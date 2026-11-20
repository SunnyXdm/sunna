// Take plain screenshots of the real Settings sheet, sharing off and on.
import {launch,work} from './browser.mjs';
const {page,close}=await launch({port:8798});
try{
  await page.goto('http://127.0.0.1:8795/ui/?count=6');
  await page.sleep(800);
  await page.click('#settings-button');
  await page.sleep(700);
  await page.shot(work+'/sheet-off.png');
  await page.click('#share-host');
  await page.sleep(2500);
  await page.shot(work+'/sheet-on.png');
  const r=await page.eval(`JSON.stringify([...document.querySelectorAll('.this-computer,.link-row,#settings-sheet,.sheet')].map(e=>{const b=e.getBoundingClientRect();return [e.className||e.id,Math.round(b.x),Math.round(b.y),Math.round(b.width),Math.round(b.height)]}))`);
  console.log(r);
}finally{await close()}
