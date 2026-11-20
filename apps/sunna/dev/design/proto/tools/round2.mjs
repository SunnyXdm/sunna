import { launch } from './cdp.mjs';
import { writeFileSync, mkdirSync } from 'node:fs';
const root=new URL('../',import.meta.url).pathname;
const {page,close}=await launch({port:9338,width:900,height:600});
const checks=[];
async function assert(name, expression){const value=await page.eval(expression);checks.push({name,ok:!!value});if(!value)throw Error(name);}
async function goto(q,s='desktop',w=900,h=600){await page.send('Emulation.setDeviceMetricsOverride',{width:w,height:h,deviceScaleFactor:1,mobile:false});await page.goto(`http://100.108.56.3:8790/${s}.html?test=0&sel=0&${q}`);}
const rect=s=>`document.querySelector('${s}').getBoundingClientRect()`;
try {
 for(const sub of ['keyboard','clipboard','video']){
  await goto(`view=menu&submenu=${sub}&still=1`);
  await assert(`${sub} anchored above control within 900×600`, `(()=>{const a=${rect('.submenu')},b=${rect(`[data-action="submenu-${sub}"]`)};return a.top>=12&&a.left>=12&&a.right<=888&&Math.abs(a.bottom-b.top+6)<1})()`);
  await assert(`${sub} has no native selects`, `!document.querySelector('select')`);
 }
 for(const surface of ['desktop','phone']) {
  const [w,h]=surface==='phone'?[390,844]:[900,600];
  let top;
  for(const q of ['state=ready','state=busy','state=key','state=offline','fail=timeout','ended=host','remove=1','permissionShown=1']){
   await goto(q+'&still=1',surface,w,h);
   const y=await page.eval(`${rect('.act')}.top`);if(top===undefined)top=y;
   await assert(`${surface} stable action top: ${q}`,`Math.abs(${rect('.act')}.top-${top})<1`);
   if(surface==='desktop')await assert(`16px status/action gap: ${q}`,`Math.abs(${rect('.act')}.top-${rect('.st-info')}.bottom-16)<1`);
  }
  for(const q of ['context=1','context=1&longNames=1&sel=1']){
   await goto(q+'&still=1',surface,w,h);
   await assert(`${surface} context anchored ${q}`,`(()=>{const a=${rect('.context')},b=${rect('.more-computer')};return a.top>=12&&a.bottom<=${h-12}&&a.left>=12&&a.right<=${w-12}&&(Math.abs(a.bottom-b.top+6)<1||Math.abs(a.top-b.bottom-6)<1)})()`);
  }
  for(const [q,expected]of [['view=settings','computer'],['view=add','computer'],['view=edit','computer'],['view=qr','computer'],['view=find','add'],['view=licenses','settings'],['view=menu&submenu=video','menu'],['view=menu','session'],['view=session&keyboard=1','session']]){
   for(const how of ['Escape','Back']){await goto(q+'&still=1',surface,w,h);if(how==='Escape')await page.press('Escape');else {await page.eval('history.back()');await page.sleep(120);}await assert(`${surface} ${how} ${q} → ${expected}`,`cur==='${expected}'&&!ui.submenu&&!ui.keyboard`);}
  }
 }
 await goto('computers=12&still=1');await assert('12-computer names scroll inside window',`(()=>{const r=${rect('.st-tabs .in')},e=document.querySelector('.st-tabs .in');return r.left>=0&&r.right<=innerWidth&&e.scrollWidth>e.clientWidth})()`);
 await goto('view=menu&longNames=1&still=1', 'desktop',1280,800);await assert('Other strip centered, labels constrained',`(()=>{const r=${rect('.other-strip')};return Math.abs((r.left+r.right)/2-innerWidth/2)<1&&[...document.querySelectorAll('.other')].every(e=>e.querySelector('span').clientWidth<=e.clientWidth)})()`);
 for(const h of [600,1000]){
  await goto('view=add&still=1','desktop',1280,h);const initial=await page.eval(`${rect('.form-screen')}.height`);
  await assert(`Form stage size ${h}`,`${initial}>=149 && ${h===1000?initial+'>=350':initial+'<=170'}`);
  await page.eval(`form.state='ready';render()`);await assert(`Form fills same box ${h}`,`Math.abs(${rect('.form-screen')}.height-${initial})<1`);
 }
 await goto('view=session&keyboard=1&still=1','phone',390,844);await assert('QWERTY letter rows 10/9/7 with Shift/Backspace',`JSON.stringify([...document.querySelectorAll('.keyrow')].slice(0,3).map(e=>[...e.querySelectorAll('[data-keyboard-key]')].filter(b=>b.dataset.keyboardKey.length===1).map(b=>b.textContent).join('')))==='["qwertyuiop","asdfghjkl","zxcvbnm"]'&&!!document.querySelector('[data-keyboard-key="Shift"]')&&!!document.querySelector('[data-keyboard-key="Backspace"]')`);
 await assert('Accessory targets remain 44px',`[...document.querySelectorAll('.accessory button')].every(e=>e.offsetWidth>=44&&e.offsetHeight>=44)`);
 for(const size of [[390,844],[844,390]]){await goto('view=session&stats=1&statsDetail=full&still=1','phone',...size);await assert('Stats capsule in viewport '+size,`(()=>{const r=${rect('.capsule')};return r.left>=16&&r.right<=innerWidth-16&&r.top>=0})()`);}
 await goto('view=computer&still=1');await page.press('F10',8);await assert('Keyboard context uses button anchor',`!!document.querySelector('.context')`);
 await goto('view=computer&still=1');await page.eval(`act('settings');act('qr-self');back()`);await assert('QR returns to Settings',`cur==='settings'`);await page.eval('back()');await assert('Settings returns to computer',`cur==='computer'`);
 await goto('view=computer&still=1','phone',390,844);await page.eval(`act('add');act('find');act('form-back');back()`);await assert('Find return does not change Add parent',`cur==='computer'`);
 await goto('view=session&stats=1&statsDetail=full&still=1','phone',390,844);
 await page.eval(`app.style.setProperty('--safe-top','44px');app.style.setProperty('--safe-bottom','34px')`);await page.eval('render()');await assert('Stats clears simulated top notch',`${rect('.capsule')}.top>=58`);
 await page.eval(`app.style.setProperty('--safe-top','0px');app.style.setProperty('--safe-bottom','21px');app.style.setProperty('--safe-left','44px')`);await page.send('Emulation.setDeviceMetricsOverride',{width:844,height:390,deviceScaleFactor:1,mobile:false});await page.eval('render()');await assert('Stats clears simulated side notch',`${rect('.capsule')}.left>=60&&${rect('.capsule')}.right<=828`);
 await page.eval(`['top','bottom','left','right'].forEach(side=>app.style.removeProperty('--safe-'+side))`);
 for(const surface of ['desktop','phone']){
  const [w,h]=surface==='phone'?[390,844]:[900,600];await goto('view=session',surface,w,h);
  await page.record(root+`shots/motions/round2-${surface}-menu.mp4`,async()=>{
   const samples=await page.eval(`new Promise(resolve=>{const frames=[];act('menu');const start=performance.now();function sample(t){frames.push({ms:performance.now()-start,opacity:+getComputedStyle(document.querySelector('.session-controls')).opacity,width:document.querySelector('.sess-shot .shot').getBoundingClientRect().width});if(frames.length<24)requestAnimationFrame(sample);else resolve(frames);}requestAnimationFrame(sample);})`);
   writeFileSync(root+`shots/${surface}-menu-timing.json`,JSON.stringify(samples,null,2));
   checks.push({name:surface+' first 100ms (six 60fps frames) hidden',ok:samples.filter(x=>x.ms<=100).every(x=>x.opacity===0) && samples.some(x=>x.ms>120&&x.opacity>0&&x.width<samples[0].width),samples:samples.slice(0,6)});
   if(!checks.at(-1).ok)throw Error('Early controls '+surface);
   await page.sleep(500);await page.eval(`act('close-menu')`);await page.sleep(90);await assert(surface+' closes controls before zoom','cur==="menu"&&+getComputedStyle(document.querySelector(".session-controls")).opacity<1');await page.sleep(1000);
  });
 }
 console.log('PASS',checks.length,'round-two checks');
}catch(e){console.error(e);checks.push({name:'Fatal',error:String(e)});process.exitCode=1;}finally{writeFileSync(root+'shots/round2.json',JSON.stringify({checks,console:page.logs.filter(x=>/EXCEPTION|error:/.test(x))},null,2));close();}
