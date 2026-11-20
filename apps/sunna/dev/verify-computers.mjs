// Reproducible visual matrix. Uses only fake keys and the supplied screen frames.
import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync} from 'node:fs';
const out=new URL('./design/shots-app/',import.meta.url).pathname;
mkdirSync(out,{recursive:true});
const {page,close}=await launch({scale:1,port:9346});
await page.send('Network.enable');await page.send('Network.setCacheDisabled',{cacheDisabled:true});
const views=['devices','wall','space','type','gallery'];
const results=[];
const mode=process.argv[2]||'all';
const connect=async()=>{const selector=await page.eval(`document.body.dataset.view==='devices'||document.body.dataset.view==='type'?'.computer.on .tile-hit':'.computer .tile-hit'`);await page.click(selector);await page.sleep(50);if(!await page.eval('!document.getElementById("launch").hidden'))throw new Error('Connect did not open');};
const escape=async()=>{await page.press('Escape');await page.sleep(400);};
const goto=async query=>{
 await page.goto(`http://127.0.0.1:8766/ui/?${query}`);
 for(let i=0;i<100;i++){if(await page.eval('document.body.dataset.pictures === "loaded"'))break;await page.sleep(100);}
 if(!await page.eval('document.body.dataset.pictures === "loaded"'))throw new Error('Pictures did not settle');
 await page.sleep(1100);
};
const shot=async name=>{await page.sleep(250);await page.shot(out+name+'.png');results.push(name);};
try {
if(mode==='all'||mode==='matrix'||mode==='refine'){
for(const [size,width,height] of [['default',980,660],['minimum',680,480],['retina',1000,650]]) {
 await page.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:size==='retina'?2:1,mobile:false});
 for(const view of (mode==='refine'?['type','space']:views))for(const count of [0,1,2,6,20]) {
  await goto(`view=${view}&count=${count}`);await shot(`${view}-${count}-${size}`);
  if(count===20){await page.eval(`document.querySelector('.view-grid,.scene,.type-nav')?.scrollTo(0,10000)`);await shot(`${view}-20-bottom-${size}`);}
 }
 for(const view of views){await goto(`view=${view}&count=6&long=1`);await shot(`${view}-long-${size}`);}
}
await page.send('Emulation.setDeviceMetricsOverride',{width:980,height:660,deviceScaleFactor:1,mobile:false});
for(const view of (mode==='refine'?[]:views))for(const state of ['idle','checking','ready','busy','wrong-key','update-needed','not-found','invalid','unreachable']){
 await goto(`view=${view}&count=1&state=${state}&no-picture=1`);await shot(`${view}-state-${state}`);
}
}
if(mode==='all'||mode==='recordings'||mode==='devices-motion'){
for(const view of (mode==='devices-motion'?['devices']:views)){
 await goto(`view=${view}&count=6`);
 await page.record(out+`${view}-connect-leave.mp4`,async()=>{await connect();await page.sleep(6000);});
 if(!await page.eval('document.getElementById("launch").hidden'))throw new Error('Leave did not return to Computers');
 await goto(`view=${view}&count=6`);
 await page.record(out+`${view}-cancel.mp4`,async()=>{await connect();await page.sleep(450);await page.click('#launch-cancel');await page.sleep(1600);});
 await goto(`view=${view}&count=6&fail=1`);
 await page.record(out+`${view}-failure.mp4`,async()=>{await connect();await page.sleep(3200);});
}
await goto('count=6');
await page.record(out+'switch-views.mp4',async()=>{for(let i=1;i<=5;i++){await page.press(String(i),2);await page.sleep(1100);}});
}
if(mode==='all'||mode==='sheets'){
for(const view of ['devices','space']){
 await goto(`count=6&view=${view}`);await page.click('#view-button');await shot(`${view}-view-menu`);await escape();
 await page.click('#add-button');await shot(`${view}-add`);
 await page.click('input[name=address]');await page.type('100.124.64.79');await page.sleep(1300);await shot(`${view}-add-ready`);
 await page.click('.find');await page.sleep(1800);await shot(`${view}-tailscale`);await escape();await escape();
 await page.click('#settings-button');await page.sleep(700);await shot(`${view}-settings`);
 await page.eval("document.querySelector('.settings-sheet .sheet-scroll').scrollTop=10000");await shot(`${view}-settings-bottom`);
 await page.click('.licenses-button');await page.sleep(300);await shot(`${view}-licenses`);await escape();await escape();
 await page.eval("(document.querySelector('.computer.on .computer-more')||document.querySelector('.computer .computer-more')).click()");await shot(`${view}-computer-menu`);
 await page.eval("[...document.querySelectorAll('.menu-item')].find(b=>b.textContent.includes('Edit')).click()");await page.sleep(300);await shot(`${view}-edit`);await page.click('.remove');await shot(`${view}-remove`);await escape();await escape();
 await page.eval("(document.querySelector('.computer.on .computer-more')||document.querySelector('.computer .computer-more')).click()");await page.eval("[...document.querySelectorAll('.menu-item')].find(b=>b.textContent.includes('QR')).click()");await page.sleep(300);await shot(`${view}-qr`);await escape();
}
}
writeFileSync(out+`browser-results-${mode}.json`,JSON.stringify({screenshots:results,logs:page.logs},null,2));
console.log(`Captured ${results.length} screenshots (${mode}). Browser exceptions: ${page.logs.filter(l=>l.startsWith('EXCEPTION')).length}`);
} finally {close();}
