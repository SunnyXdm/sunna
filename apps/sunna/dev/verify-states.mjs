// Additional inventory states and keyboard/action assertions; fake credentials only.
import {launch} from './cdp.mjs';
import {writeFileSync} from 'node:fs';
const out=new URL('./design/shots-app/',import.meta.url).pathname;
const {page,close}=await launch({port:9347,scale:1});const captures=[];
const go=async(q='count=1')=>{await page.goto('http://127.0.0.1:8766/ui/?'+q);await page.sleep(1800);};
const shot=async n=>{await page.sleep(400);await page.shot(out+n+'.png');captures.push(n);};
const esc=async()=>{await page.press('Escape');await page.sleep(400);};
const set=async(selector,value)=>page.eval(`(()=>{const e=document.querySelector(${JSON.stringify(selector)});e.value=${JSON.stringify(value)};e.dispatchEvent(new Event('input',{bubbles:true}));})()`);
const override=async(name,fn)=>page.eval(`window.__sunnaMock.commands.${name}=${fn}`);
const assert=async(expr,message)=>{if(!await page.eval(expr))throw Error(message);};
try {
if(process.argv[2]!=='actions')for(const view of ['devices','wall','space','type','gallery']){
 for(const [name,extra,change] of [
  ['identity-changed','',`c.state='identity-changed';c.detail="This isn't the computer you added, or it was set up again. If you reinstalled it, add it again with its new link."`],
  ['not-fully-verified','',`c.unverified=true`],
  ['ready-no-latency','',`c.rtt_ms=null`],
  ['busy-with-viewer','',`c.state='busy';c.viewer="Priya's MacBook Pro"`],
  ['never-connected','&never=1&no-picture=1',`c.state='unreachable'`],
 ]){
  await go(`count=1&view=${view}${extra}`);
  await page.eval(`{const old=window.__sunnaMock.commands.machine_statuses;window.__sunnaMock.commands.machine_statuses=async()=>{const all=await old();for(const c of Object.values(all)){${change}}return all;};}`);
  await page.press('r',2);await page.sleep(500);await shot(`${view}-state-${name}`);
 }
}
if(process.argv[2]!=='actions')for(const kind of ['laptop','desktop','mini','server','vm','unknown']){await go(`view=devices&count=1&kind=${kind}`);await shot(`device-kind-${kind}`);}
for(const view of ['devices','space']){
 await go(`count=1&view=${view}`);if(process.argv[2]!=='actions'){await page.click('#add-button');await page.sleep(400);
 for(const state of ['checking','busy','wrong-key','update-needed','unreachable','not-found','invalid']){
  await override('check_machine',`async()=>({state:${JSON.stringify(state)},detail:${JSON.stringify({'checking':'Looking…','busy':'Someone is connected to it right now.','wrong-key':"Sunna is there, but the key doesn't match.",'update-needed':'This computer runs a different version of Sunna.','unreachable':'Nothing answered at this address.','not-found':"Couldn't find that computer.",'invalid':"That doesn't look like an address."}[state])},os:'Arch Linux',device:'desktop'})`);
  await set('input[name=address]','100.124.64.79');await page.sleep(550);await shot(`${view}-form-${state}`);
 }
 await override('add_machine',`async()=>{throw Error('Could not save the computer.');}`);await page.click('.submit');await shot(`${view}-form-save-error`);
 await set('input[name=key]','demonstration-only');await page.click('.reveal');await shot(`${view}-key-revealed`);await esc();
 for(const [name,fn] of [['empty','async()=>({machines:[]})'],['error',`async()=>{throw Error('Tailscale is not running.');}`]]){
  await override('scan_tailscale',fn);await page.click('#add-button');await page.sleep(400);await page.click('.find');await page.sleep(400);await shot(`${view}-tailscale-${name}`);await esc();await esc();
 }
 await override('qr_code',`async()=>{throw Error('fixture');}`);
 await page.eval("document.querySelector('.computer-more').click()");await page.eval("[...document.querySelectorAll('.menu-item')].find(e=>e.textContent.includes('QR')).click()");await shot(`${view}-qr-error`);await esc();
 }
 await page.click('#settings-button');await page.sleep(600);
 await page.eval("document.querySelector('[data-view-choice=wall]').click()");await assert("document.body.dataset.view==='wall'",'Settings did not apply the view');
 await assert("window.__sunnaMock.commands.get_settings().view==='wall'",'Settings did not persist the view');await esc();
 await page.press('1',2);await page.sleep(600);await page.eval("document.querySelector('.computer .tile-hit').focus()");await page.press('e',2);await page.sleep(400);
 await page.eval("{const e=document.querySelector('select[name=kind]');e.value='mini';e.dispatchEvent(new Event('change'));}");await page.click('.submit');await page.sleep(700);await assert("document.querySelector('.computer.mini')!==null",'Kind override not applied');
 await page.eval("document.querySelector('.computer .tile-hit').focus()");await page.press('Delete');await page.sleep(1100);await page.click('.confirm');for(let i=0;i<30&&await page.eval("document.querySelectorAll('.computer').length");i++)await page.sleep(100);await assert("document.querySelectorAll('.computer').length===0",'Keyboard removal failed');await shot(`${view}-removed-toast`);
}
await go('count=1&access=no');await page.click('#settings-button');await page.sleep(700);await shot('access-denied');await page.click('.access-allow');await page.sleep(900);await shot('access-reset');
writeFileSync(out+'browser-results-states.json',JSON.stringify({screenshots:captures,logs:page.logs,assertions:'view choice applies and persists; kind override saved; keyboard edit and delete'},null,2));console.log(`Captured ${captures.length} additional states; assertions passed.`);
} catch(e){await page.shot(out+'state-test-failure.png');console.log(page.logs);throw e;}finally{close();}
