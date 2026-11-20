import {launch} from './cdp.mjs';
import {writeFileSync} from 'node:fs';
const {page,close}=await launch({port:9348,scale:1});const out=new URL('./design/shots-app/',import.meta.url).pathname;const results=[];
const assert=async(e,m)=>{if(!await page.eval(e))throw Error(m);};
const go=async q=>{await page.goto('http://127.0.0.1:8766/ui/?'+q);await page.sleep(1600);};
try{
await go('count=1');await assert("document.body.dataset.view==='gallery' && window.__sunnaMock.commands.get_settings().view==='gallery'",'Expected Gallery by default');
await go('count=1&view=unknown');await assert("document.body.dataset.view==='gallery'",'Invalid saved view did not fall back to Gallery');
for(const view of ['devices','wall','space','type','gallery']){
 await go(`count=20&view=${view}`);await page.eval('document.activeElement.blur()');await page.press('ArrowLeft');const seen=new Set();
 for(let i=0;i<20;i++){seen.add(await page.eval('document.activeElement.closest("[data-id]")?.dataset.id'));await page.press('ArrowRight');}
 await assert("document.activeElement.matches(':focus-visible') && getComputedStyle(document.activeElement).outlineStyle==='none'",view+' computer has a keyboard ring');
 if(view==='wall'){
  await page.eval(`Promise.all(document.getAnimations().filter(a=>a.transitionProperty==='filter').map(a=>a.finished.catch(()=>{})))`);
  await assert("getComputedStyle(document.activeElement.querySelector('.scr')).filter==='none' && getComputedStyle(document.querySelector('.computer:not(:focus-within) .scr')).filter==='brightness(0.42)'",'Wall keyboard selection does not dim its neighbours');
 }
 if(seen.size!==20||seen.has(undefined))throw Error(`${view}: keyboard did not reach all 20 computers`);
 await page.press('F10',8);await assert('!document.getElementById("menu").hidden',`${view}: keyboard context menu missing`);
 await assert(`(()=>{const s=getComputedStyle(document.activeElement),[r,g,b]=s.outlineColor.match(/[0-9.]+/g).map(Number);return s.outlineStyle==='solid' && parseFloat(s.outlineWidth)<=2 && Math.max(r,g,b)-Math.min(r,g,b)<=3})()`,view+' menu keyboard focus must have a thin neutral ring');
 await page.press('Escape');await page.press('e',2);await page.sleep(400);await assert('!!document.querySelector("select[name=kind]")',`${view}: keyboard Edit missing`);await page.press('Escape');await page.sleep(400);
 await go(`count=0&view=${view}`);await page.press('n',2);await page.sleep(400);
 await page.eval(`{const e=document.querySelector('input[name=address]');e.value='sunna://100.124.64.79?key='+window.__sunnaMock.commands.get_settings().key;e.dispatchEvent(new Event('input',{bubbles:true}));}`);
 await page.sleep(900);await assert(`document.querySelector('input[name=address]').value==='100.124.64.79'`,'Link was not parsed');await page.click('.submit');await page.sleep(1500);await assert('document.querySelectorAll(".computer").length===1',`${view}: add did not create a computer`);await page.shot(out+`${view}-added.png`);results.push(`${view}: keyboard reaches all 20; context menu and Edit; add by pasted link`);
}
await page.send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:'reduce'}]});await go('count=6&view=space');await assert(`getComputedStyle(document.querySelector('.fl')).animationName==='none'`,'Reduced motion still floats');results.push('Reduced motion disables floating animation');
writeFileSync(out+'browser-results-actions.json',JSON.stringify({results,logs:page.logs},null,2));console.log('All five views passed keyboard, add, and reduced-motion checks.');
}catch(e){await page.shot(out+'action-test-failure.png');console.log(page.logs);throw e;}finally{close();}
