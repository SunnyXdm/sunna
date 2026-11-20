// Recovery and compatibility checks which don't fit the visual matrix.
import {launch} from './cdp.mjs';
import {writeFileSync} from 'node:fs';
const {page,close}=await launch({scale:1,port:9359});
const results=[];
const go=async q=>{await page.goto('http://127.0.0.1:48987/ui/?count=1&view=devices&'+q);await page.sleep(1000);};
const assert=async(e,m)=>{if(!await page.eval(e))throw Error(m);results.push(m);};
try{
 await go('host-update=available');
 await page.eval(`{const hosts=window.__sunnaMock.commands.machine_statuses;window.__sunnaMock.commands.machine_statuses=async()=>{const all=await hosts();for(const c of Object.values(all))c.update.state.available=null;return all;};const app=window.__sunnaMock.commands.update_status;window.__sunnaMock.commands.update_status=()=>({...app(),latest_version:'0.2.0'});}`);
 await page.press('r',2);await page.sleep(2200);
 await page.click('#app-update-notice');
 await assert(`!document.querySelector('.computer-update') && document.querySelector('#menu .menu-item').textContent==='Update archlinux…'`,'Newest known release finds an older host even when its own check has no available version.');
 await go('host-update=done');
 await page.eval(`{const old=window.__sunnaMock.commands.machine_statuses;window.__sunnaMock.commands.machine_statuses=async()=>{const all=await old();for(const c of Object.values(all))c.update.state.failed=['0.1.1'];return all;};}`);
 await page.press('r',2);await page.sleep(350);
 await assert(`document.querySelector('.cap .computer-update').textContent==='Updated to 0.2.0'`,'A historical failed host release does not hide a later successful update.');
 await go('app-update=up-to-date');
 await page.eval(`{const old=window.__sunnaMock.commands.update_status;window.__sunnaMock.commands.update_status=()=>({...old(),version:'0.3.0',current:'0.3.0',latest_version:'0.3.0',failed:['0.2.0']});}`);
 await page.click('#settings-button');await page.sleep(700);
 await assert(`document.querySelector('.update-state').textContent==='Up to date' && document.querySelector('.update-explanation').hidden`,'A historical failed app release does not mark the new working version as failed.');
 await page.eval(`window.__sunnaMock.commands.update_policy=()=>{throw 'Could not save the policy: permission denied.';};const e=document.querySelector('.app-update-policy');e.value='off';e.dispatchEvent(new Event('change'));`);await page.sleep(200);
 await assert(`document.querySelector('.app-update-policy').value==='notify' && document.querySelector('.update-explanation').textContent.includes('permission denied')`,'A rejected app policy restores the previous choice and shows the reason.');
 await go('host-update=available');
 await page.eval(`window.__sunnaMock.commands.host_update=()=>{throw 'Connection timed out.';};document.querySelector('#app-update-notice').click();document.querySelector('#menu .menu-item').click()`);await page.sleep(600);
 await assert(`document.querySelector('.host-install-command').hidden && document.querySelector('.host-update-detail').textContent.includes('timed out')`,'An unreachable host is not misidentified as an unsupported installation.');
 await go('state=update-needed&host-update=available');
 await page.eval(`{const old=window.__sunnaMock.commands.machine_statuses;window.__sunnaMock.commands.machine_statuses=async()=>{const all=await old();for(const c of Object.values(all))c.detail='That computer runs a newer Sunna. Update this one.';return all;};}`);
 await page.press('r',2);await page.sleep(250);await page.click('.cap .go');await page.sleep(250);
 await assert(`!document.querySelector('.host-update-sheet') && document.getElementById('toast').textContent.includes('Update this one')`,'A newer host protocol asks to update this app, preserving the direction of the mismatch.');
 writeFileSync(new URL('./design/shots-app/updates-edge-results.json',import.meta.url),JSON.stringify({results,logs:page.logs},null,2));
 console.log(`PASS: ${results.length} compatibility and recovery checks.`);
}finally{await close();}
