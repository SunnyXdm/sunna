import {launch} from './cdp.mjs';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
const root=new URL('../',import.meta.url).pathname, shots=root+'shots/';mkdirSync(shots+'raw',{recursive:true});
const cases=JSON.parse(readFileSync(root+'tools/cases.json'));
const {page,close}=await launch({width:1280,height:800,scale:1,port:9335});
await page.send('Network.enable');await page.send('Network.setCacheDisabled',{cacheDisabled:true});
const manifest=[],errors=[],issues=[];
function slug(s){return s.replace(/[^a-zA-Z0-9.-]/g,'-');}
async function capture(surface,label,query,w,h,scale=1){await page.send('Emulation.setDeviceMetricsOverride',{width:w,height:h,deviceScaleFactor:scale,mobile:false});let url=`http://100.108.56.3:8790/${surface}.html?${query}&still=1${query.includes('testOpen')||label.startsWith('A15')?'':'&test=0'}`;let start=page.logs.length;await page.goto(url);await page.sleep(35);let state=await page.eval(`({view:cur, horizontal:document.body.scrollWidth>innerWidth, count:C.length, nativeSelects:document.querySelectorAll('select').length, title:current().name})`);let logs=page.logs.slice(start).filter(x=>/EXCEPTION|error:/.test(x));if(logs.length)errors.push({label,url,logs});if(state.nativeSelects)issues.push({label,issue:'native select'});if(state.horizontal)issues.push({label,issue:'horizontal overflow'});const file=`raw/${surface}-${String(manifest.length).padStart(3,'0')}-${slug(label)}-${w}.png`;await page.shot(shots+file);manifest.push({surface,label:label+' · '+w+'×'+h,url,file,w,h,scale});}
try{
for(const c of cases){const surface=c.id.startsWith('A')?'phone':'desktop';for(const [w,h,s]of surface==='desktop'?[[1280,800,1],[900,600,1]]:[[390,844,2]])await capture(surface,c.id+' '+c.label,'case='+c.id,w,h,s);if(manifest.length%20<3)console.log('captured',manifest.length,c.id);}
for(const surface of ['desktop','phone']){
 const [w,h,s]=surface==='desktop'?[1280,800,1]:[390,844,2];
 for(const count of [0,1,2,5,12])for(const view of ['computer','overview'])await capture(surface,`${view}-${count}`,`view=${view}&computers=${count}`,w,h,s);
 for(const state of ['update-client','never','idle','busy-anonymous'])await capture(surface,'state-'+state,`state.arch=${state}`,w,h,s);
 for(const sel of [1,2,3,4,10])await capture(surface,'computer-'+sel,`computers=12&sel=${sel}`,w,h,s);
 for(const fail of ['unexpected','crashed','timeout','host','replaced','key','busy','busy-anonymous','decoder','version','unreachable','unanswered','spawn','already','missing','resolution','invalid','raw'])await capture(surface,'failure-'+fail,`fail=${fail}`,w,h,s);
 for(const ended of ['unexpected','crashed','timeout','host','replaced','key','busy','busy-anonymous','decoder','version','unreachable','raw'])await capture(surface,'ended-'+ended,`ended=${ended}`,w,h,s);
 for(const state of ['update-client','invalid-ipv6','invalid-tail','invalid-port','invalid-key','empty'])await capture(surface,'form-'+state,`view=add&form=${state}`,w,h,s);
 for(const scroll of [450,850,1250])await capture(surface,'settings-'+scroll,`view=settings&scroll=${scroll}`,w,h,s);
 if(surface==='phone')for(const scroll of [500,1000,1450])await capture(surface,'menu-'+scroll,`view=menu&scroll=${scroll}`,w,h,s);
 if(surface==='desktop')for(const state of ['signed-out','output'])await capture(surface,'tailscale-'+state,`view=find&discovery=${state}`,w,h,s);
}
for(const view of ['session','menu'])await capture('phone','landscape-'+view,`view=${view}`,844,390,2);
for(const [w,h]of [[820,560],[1920,1200]])for(const view of ['computer','overview','menu','settings','add'])await capture('desktop',`resize-${view}`,`view=${view}&computers=12`,w,h);
await capture('phone','wide-frame','view=computer',1000,1000);
await capture('phone','wide-add','case=A17.3',1000,1000);
for(const surface of ['desktop','phone']){const [w,h]=surface==='desktop'?[900,600]:[390,844];for(const [label,q]of [['long-name','longNames=1&sel=1'],['long-strip','longNames=1&view=menu'],['anchored-context','context=1&longNames=1&sel=1'],['test-long-names','longNames=1&testOpen=true']])await capture(surface,label,q,w,h);}
for(const submenu of ['keyboard','clipboard','video'])await capture('desktop','minimum-'+submenu,'view=menu&submenu='+submenu,820,560);
// Contact sheets are rendered HTML: image assets are untouched, labels are drawn into PNG.
for(const surface of ['desktop','phone']){const list=manifest.filter(x=>x.surface===surface),n=surface==='desktop'?6:10,cols=surface==='desktop'?3:5,tw=surface==='desktop'?600:300,th=surface==='desktop'?400:674;
for(let i=0;i<list.length;i+=n){const batch=list.slice(i,i+n),name=surface+'-'+String(i/n+1).padStart(2,'0')+'.png';const html=`<!doctype html><meta charset="utf-8"><link rel="icon" href="data:,"><style>*{box-sizing:border-box}body{margin:0;background:#303034;color:white;font:12px system-ui}.grid{display:grid;grid-template-columns:repeat(${cols},${tw}px)}.tile{height:${th}px;padding:8px}.label{height:27px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.image{height:${th-43}px;background:#0b0b0c;display:flex;align-items:center;justify-content:center}img{width:100%;height:100%;object-fit:contain}</style><div class="grid">${batch.map(x=>`<div class="tile"><div class="label">${x.label.replace(/&/g,'&amp;').replace(/</g,'&lt;')}</div><div class="image"><img src="../shots/${x.file}"></div></div>`).join('')}</div>`;writeFileSync(root+'tools/contact.html',html);await page.send('Emulation.setDeviceMetricsOverride',{width:tw*cols,height:th*Math.ceil(batch.length/cols),deviceScaleFactor:1,mobile:false});await page.goto('http://100.108.56.3:8790/tools/contact.html?sheet='+surface+i);await page.eval('Promise.all([...document.images].map(i=>i.decode()))');await page.eval('new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)))');await page.sleep(80);await page.shot(shots+name);}}
writeFileSync(shots+'manifest.json',JSON.stringify(manifest,null,2));writeFileSync(shots+'verification.json',JSON.stringify({loads:manifest.length,errors,issues},null,2));console.log('DONE',manifest.length,'loads;',errors.length,'console errors;',issues.length,'issues');
}finally{close();}
