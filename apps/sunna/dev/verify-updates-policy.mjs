import {launch} from './cdp.mjs';
import {writeFileSync} from 'node:fs';
const {page,close}=await launch({scale:1,port:9360});
const out=new URL('./design/shots-app/',import.meta.url).pathname;
const results=[];
try{
 for(const [size,width,height] of [['default',980,660],['minimum',680,480]])for(const view of ['devices','space']){
  await page.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
  await page.goto(`http://127.0.0.1:48987/ui/?count=6&view=${view}&host-update=available`);await page.sleep(1000);
  await page.eval(`(document.querySelector('.computer.on .computer-more')||document.querySelector('.computer .computer-more')).click()`);
  await page.eval(`[...document.querySelectorAll('.menu-item')].find(e=>e.textContent.includes('Edit…')).click()`);await page.sleep(800);
  await page.eval(`document.querySelector('.host-policy-field').scrollIntoView({block:'center'})`);await page.sleep(200);
  const visible=await page.eval(`(()=>{const sheet=document.querySelector('.machine-sheet').getBoundingClientRect();return ['.sheet-header','.host-update-policy','.sheet-footer'].every(s=>{const r=document.querySelector('.machine-sheet '+s).getBoundingClientRect();return r.top>=sheet.top && r.bottom<=sheet.bottom;});})()`);
  if(!visible)throw Error(`${view} ${size}: title, policy or actions clipped`);
  await page.shot(out+`updates-policy-${view}-${size}.png`);results.push(`${view} ${size}: title, policy and actions visible; fields scroll.`);
 }
 writeFileSync(out+'updates-policy-results.json',JSON.stringify({results,logs:page.logs},null,2));console.log('PASS: Edit policy at both sizes, dark and Space.');
}finally{await close();}
