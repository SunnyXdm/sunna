import {execFileSync} from 'node:child_process';
import {launch,work} from './browser.mjs';
import {readFileSync,writeFileSync} from 'node:fs';

const {page,close}=await launch({width:980,height:660,scale:2,port:8796});
const out='docs/images/recordings/';
const go=async q=>{await page.goto('http://127.0.0.1:8795/ui/?count=6&view=gallery&time=12:00&'+q);await page.sleep(1250)};
const shot=async(name,clip)=>{
  const {data}=await page.send('Page.captureScreenshot',{format:'webp',quality:95,...(clip?{clip}:{})});
  writeFileSync(out+name+'.webp',Buffer.from(data,'base64'));
};
try{
 // The original Kate + htop fixture includes a login in htop. Use the editor pane.
 execFileSync('magick',['apps/sunna/dev/design/frames/01-plasma-rust.jpg','-crop','884x648+42+115','+repage',work+'/editor.png']);
 const data=name=>'data:image/png;base64,'+readFileSync(work+'/'+name+'.png').toString('base64');
 await page.send('Page.addScriptToEvaluateOnNewDocument',{source:`Object.defineProperty(window,'__sunnaMock',{configurable:true,set(mock){Object.defineProperty(this,'__sunnaMock',{value:mock});const original=mock.commands.machine_statuses;mock.commands.machine_statuses=async(...args)=>{const result=await original(...args);if(result.c3)Object.assign(result.c3,{width:1280,height:862});return result}}});const original=window.fetch.bind(window);window.fetch=(url,...args)=>original(String(url).endsWith('04-xfce-docs.jpg')?(window.finished?${JSON.stringify(data('after'))}:${JSON.stringify(data('before'))}):url,...args)`});
 if(process.argv.includes('features')){
 await go('');
 for(const name of ['gallery','devices','wall','space','type']){
   await page.click('#view-button');await page.click('[data-view-choice="'+name+'"]');
   await page.move(1,1);await page.sleep(1000);await shot(name);
 }
 // A smaller two-computer window keeps both complete tiles behind the menu.
 await page.send('Emulation.setDeviceMetricsOverride',{width:720,height:400,deviceScaleFactor:2,mobile:false});
 for(const [mode,name] of [['available','updates-menu'],['updating','progress'],['done','finished']]){
   await page.goto('http://127.0.0.1:8795/ui/?count=2&view=gallery&host-update='+mode);
   await page.sleep(1300);
   if(mode==='available'){await page.click('#app-update-notice');await page.sleep(300)}
   await shot(name,{x:0,y:0,width:720,height:352,scale:1});
 }
 }else{
 await go('');await page.sleep(1800);await page.shot(work+'/plain.png');
 const hit='.computer[data-id="c3"] .tile-hit';
 const {x,y}=await page.center(hit);
 await page.eval(`const cursor=document.createElement('div');cursor.style.cssText='pointer-events:none;position:fixed;z-index:99999;left:0;top:0;width:19px;height:25px;transform:translate(820px,570px)';cursor.innerHTML='<svg viewBox="0 0 19 25"><path fill="#202124" stroke="white" stroke-width="1.4" d="M2 2v19l5-5 4 8 3-2-4-8h7z"/></svg>';document.body.append(cursor);addEventListener('mousemove',e=>cursor.style.transform='translate('+e.clientX+'px,'+e.clientY+'px)');`);
 // Hold the mock connection until the captured viewer has disconnected.
 await page.eval(`const mock=window.__sunnaMock;mock.commands.connect=({id})=>{mock.session=id;setTimeout(()=>mock.emit('session-progress',{id,phase:'video'}),700);setTimeout(()=>mock.emit('session-progress',{id,phase:'open'}),1400)}`);
 await page.record(work+'/open.mp4',async()=>{
   await page.sleep(1000);
   for(let i=1;i<=30;i++){let t=(1-Math.cos(Math.PI*i/30))/2;await page.move(820+(x-820)*t,570+(y-570)*t);await page.sleep(24)}
   await page.sleep(500);await page.shot(work+'/hover.png');await page.click(hit);await page.sleep(1300);
 },{scale:2});
 await page.record(work+'/return.mp4',async()=>{
   await page.move(820,570);
   await page.eval(`window.finished=true;window.__sunnaMock.emit('session-ended',{id:'c3',opened:true,reason:null})`);
   await page.sleep(1400);
   for(const type of ['mousePressed','mouseReleased'])await page.send('Input.dispatchMouseEvent',{type,x:820,y:570,button:'left',clickCount:1});
   await page.sleep(500);
 },{scale:2});
 }
}finally{await close()}
