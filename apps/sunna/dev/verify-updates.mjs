// Reproducible update state matrix and command/flow assertions. Fake hosts only.
import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync,readdirSync} from 'node:fs';
const out=new URL('./design/shots-app/',import.meta.url).pathname;
const base=process.env.SUNNA_MOCK_URL || 'http://127.0.0.1:48987';
const {page,close}=await launch({scale:1,port:9357});
const captures=[],results=[];
const views=['devices','wall','space','type','gallery'];
const hosts=['available','updating','done','failed','unsupported','unmanaged'];
const apps=['up-to-date','checking','downloading','ready','failed','unmanaged'];
const assert=async(e,m)=>{if(!await page.eval(e))throw Error(m);};
const go=async q=>{await page.goto(`${base}/ui/?count=6&${q}`);await page.sleep(850);};
const shot=async n=>{await page.shot(out+n+'.png');captures.push(n);};
const settings=async()=>{await page.click('#settings-button');await page.sleep(550);await page.eval(`document.getElementById('updates-settings').scrollIntoView({block:'center'})`);await page.sleep(250);};
const toolbarHost=async(name)=>{
 await page.click('#app-update-notice');
 await page.eval(`[...document.querySelectorAll('#menu .menu-item')].find(e=>e.textContent===${JSON.stringify('Update '+name+'…')}).click()`);
 await page.sleep(650);
};
const openHost=async()=>{
 await page.eval(`(document.querySelector('.computer.on .computer-more') || document.querySelector('.computer .computer-more')).click()`);
 await page.eval(`[...document.querySelectorAll('.menu-item')].find(e=>e.textContent.includes('Update this computer')).click()`);
 await page.sleep(650);
};
try{
 await page.send('Network.enable');await page.send('Network.setCacheDisabled',{cacheDisabled:true});
 for(const [size,width,height] of (process.argv[2]==='flow'?[]:[['default',980,660],['minimum',680,480]])){
  await page.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
  for(const view of views)for(const mode of hosts){
   await go(`view=${view}&host-update=${mode}&app-update=ready`);
   const label={updating:'Updating…',done:'Updated to 0.2.0',failed:'Update failed'}[mode];
   const labels=await page.eval(`[...document.querySelectorAll('.computer-update')].map(e=>e.textContent)`);
   if(label ? !labels.length || labels.some(text=>text!==label) : labels.length)throw Error(view+' '+mode+' tile labels: '+labels);
   await assert(`!document.getElementById('app-update-notice').hidden`,'Missing restart notice');
   if(mode==='failed')await assert(`[...document.querySelectorAll('.computer-update')].some(e=>{const r=e.getBoundingClientRect();return e.title && document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)===e})`,'Failure tooltip has no pointer target in '+view);
   if(view==='devices' && label)await assert(`(()=>{const b=document.querySelector('.cap .update-link'),r=b.getBoundingClientRect();return r.bottom<=document.querySelector('.pips').getBoundingClientRect().top && document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)===b})()`,'Selected update action clipped or covered');
   await shot(`updates-host-${view}-${mode}-${size}`);
  }
  for(const view of ['devices','space'])for(const mode of apps){
   await go(`view=${view}&app-update=${mode}`);await settings();
   await assert(`!!document.querySelector('.update-state').textContent`,'Missing app state');
   await shot(`updates-app-${view}-${mode}-${size}`);
  }
  for(const view of ['devices','space'])for(const mode of ['available','unsupported','unmanaged']){
   await go(`view=${view}&host-update=${mode}`);await openHost();
   await shot(`updates-confirm-${view}-${mode}-${size}`);
  }
 }
 await page.send('Emulation.setDeviceMetricsOverride',{width:980,height:660,deviceScaleFactor:1,mobile:false});
 await go('view=devices&host-update=available&app-update=ready');
 await page.click('#app-update-notice');
 await assert(`[...document.querySelectorAll('#menu .menu-item')].map(b=>b.textContent).join('|')==='Restart Sunna to update to 0.2.0|Update debian…|Update thinkpad…|Update studio…|Update mumbai…|Update macbook…|Update windows…'`,'Toolbar entries or order');
 await page.shot(out+'updates-toolbar.png');
 await page.click('#menu .menu-item');await page.sleep(200);
 await assert(`window.__sunnaMock.commands.update_status().pending===null`,'Restart did not apply staged release');
 await go('view=devices&host-update=available&app-update=ready');
 await page.record(out+'updates-host-flow.mp4',async()=>{
  await toolbarHost('thinkpad');await page.sleep(1200);await page.click('.host-update-sheet .confirm');
  await page.sleep(1500);await shot('updates-flow-updating');
  await page.sleep(7500);await shot('updates-flow-done');
 });
 await assert(`document.querySelector('.cap .computer-update').textContent==='Updated to 0.2.0'`,'Request acknowledgement mistaken for completion, or completion missing');
 results.push('Confirmed host request → updating → later probe reports new running version.');
 await page.click('.cap .go');await page.sleep(250);
 await assert(`document.getElementById('app-update-notice').hidden`,'Restart notice shown during connect');
 await page.press('Escape');for(let i=0;i<50;i++){if(await page.eval(`!document.getElementById('app-update-notice').hidden`))break;await page.sleep(100);}
 await assert(`!document.getElementById('app-update-notice').hidden`,'Restart notice not restored');
 results.push('Restart notice hidden throughout connect, restored on cancel.');
 await go('view=space&host-update=available&host-result=failed');await openHost();await page.click('.host-update-sheet .confirm');await page.sleep(1000);
 await assert(`document.querySelector('.computer-update').textContent==='Update failed' && document.querySelector('.computer-update').title.includes('disk is full')`,'Host error reason missing from computer');
 await shot('updates-request-failed');await openHost();await assert(`document.querySelector('.host-update-detail').textContent.includes('disk is full')`,'Failure reason missing in update sheet');results.push('Host command failure stays on computer with the actual reason.');
 await go('view=type&host-update=available');
 await assert(`[...document.querySelectorAll('.type-name sup')].some(e=>e.firstChild && e.textContent.startsWith('4 ms'))`,'Type brief lost');
 await page.eval(`document.querySelector('.computer-more').click()`);
 await page.eval(`[...document.querySelectorAll('.menu-item')].find(e=>e.textContent.includes('Edit…')).click()`);await page.sleep(900);
 await assert(`!document.querySelector('.host-policy-field').hidden`,'Host policy missing');
 await page.eval(`{const e=document.querySelector('.host-update-policy');e.value='auto';e.dispatchEvent(new Event('change'));}`);await page.sleep(400);
 await assert(`document.querySelector('.host-policy-hint').textContent==='Saved on this computer.'`,'Host policy not saved');
 await page.eval(`document.querySelector('.host-policy-field').scrollIntoView({block:'center'})`);await shot('updates-host-policy');
 results.push('Type brief preserved; Edit policy persisted through host_update.');
 await go('view=devices&app-update=up-to-date');await settings();await page.click('.update-check');await page.sleep(350);
 await assert(`document.querySelector('.update-state').textContent.includes('Checking')`,'Check phase missing');
 await page.sleep(2000);await assert(`document.querySelector('.update-state').textContent.includes('Downloading')`,'Download phase missing');
 await page.sleep(2200);await assert(`!document.querySelector('.update-restart').hidden`,'Ready action missing');
 await page.eval(`{const e=document.querySelector('.app-update-policy');e.value='off';e.dispatchEvent(new Event('change'));}`);await page.sleep(400);
 await assert(`window.__sunnaMock.commands.update_status().policy==='off'`,'App policy not saved');
 for(const choice of ['auto','notify']){
  await page.eval(`{const e=document.querySelector('.app-update-policy');e.value='${choice}';e.dispatchEvent(new Event('change'));}`);await page.sleep(400);
  await assert(`window.__sunnaMock.commands.update_status().policy==='${choice}' && document.querySelector('.app-update-policy').value==='${choice}'`, 'Policy did not round-trip '+choice);
 }
 await assert(`[...document.querySelector('.app-update-policy').options].map(o=>o.text).join('|')==='Automatically|Ask first|Never'`,'Unexpected app update choices');
 await assert(`document.querySelector('.pictures-mode [aria-checked=true]').textContent==='Sharp'`,'Expected Sharp default');
 for(const mode of ['blurred','off','sharp']){
  await page.eval(`document.querySelector('.pictures-mode [data-value=${mode}]').click()`);
  await assert(`document.querySelector('.pictures-mode [aria-checked=true]').dataset.value==='${mode}' && window.__sunnaMock.commands.get_settings().pictures==='${mode}'`,'Picture selection did not persist '+mode);
 }
 results.push('App check → download → ready; policy saved; Screen pictures preserved.');
 await go('view=devices&state=update-needed&host-update=available');await page.click('.cap .go');await page.sleep(700);
 await assert(`!document.querySelector('.host-update-sheet .confirm').disabled`,'Protocol-old managed host cannot update');
 results.push('Needs an update offers the same authenticated update confirmation.');
 await page.send('Emulation.setDeviceMetricsOverride',{width:680,height:480,deviceScaleFactor:1,mobile:false});
 await page.goto(base+'/ui/?count=20&host-update=available');await page.sleep(1000);await page.click('#app-update-notice');
 await assert(`document.querySelectorAll('#menu .menu-item').length===20 && document.getElementById('menu').getBoundingClientRect().bottom<=innerHeight`,'Host-only menu overflows minimum window');
 await page.eval("document.querySelector('#menu .menu-item:last-child').scrollIntoView({block:'nearest'})");
 await page.click('#menu .menu-item:last-child');await page.sleep(500);
 await assert(`document.querySelector('.host-update-sheet h2').textContent==='Update thinkpad 20?'`,'Last host update unreachable');
 const exceptions=page.logs.filter(l=>l.startsWith('EXCEPTION'));
 if(exceptions.length)throw Error(exceptions.join('\n'));
 writeFileSync(out+'updates-results.json',JSON.stringify({captures:readdirSync(out).filter(n=>/^updates-(host|app|confirm)-.*-(default|minimum)\.png$/.test(n)),flowCaptures:captures,results,logs:page.logs},null,2));
 console.log(`PASS: ${captures.length} screenshots; ${results.length} flow assertions; no JavaScript exceptions.`);
}finally{close();}
