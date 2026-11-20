import {launch,work} from './browser.mjs';
import {writeFileSync} from 'node:fs';
const {page,close}=await launch({port:8798,scale:1});
const checks=[];
try {
 let shift=0;
 const outside=[];
 page.on('PerformanceTimeline.timelineEventAdded',({event})=>{
   const entry=event.layoutShiftDetails;
   if(entry && !entry.hadRecentInput)shift+=entry.value;
 });
 page.on('Network.requestWillBeSent',({request})=>{
   if(/^https?:/.test(request.url) && new URL(request.url).origin!=='http://127.0.0.1:8797')outside.push(request.url);
 });
 await page.send('PerformanceTimeline.enable',{eventTypes:['layout-shift']});
 await page.send('Network.enable');
 for (const width of [1440,390]) for(const mode of ['on','off','reduced']){
  await page.send('Emulation.setDeviceMetricsOverride',{width,height:width===390?844:900,deviceScaleFactor:1,mobile:false});
  await page.send('Emulation.setEmulatedMedia',{features:[{name:'prefers-reduced-motion',value:mode==='reduced'?'reduce':'no-preference'}]});
  await page.send('Emulation.setScriptExecutionDisabled',{value:mode==='off'});
  shift=0; outside.length=0;
  await page.goto('http://127.0.0.1:8797/');
  await page.eval('document.fonts.ready.then(()=>true)');await page.sleep(350);
  const height=await page.eval('document.documentElement.scrollHeight');
  for(let y=0;y<height;y+=650) {await page.eval('scrollTo(0,'+y+')');await page.sleep(50)}
  await page.eval('scrollTo(0,0)');await page.sleep(200);
  const state=await page.eval(`(()=>{const v=document.querySelector('video');return {overflow:document.documentElement.scrollWidth>innerWidth,paused:v.paused,controls:v.controls,media:v.currentSrc}})()`);
  Object.assign(state,{shift,outside:[...outside]});
  if(state.overflow || state.outside.length || state.shift>0 || (mode!=='on' && (!state.paused||!state.controls)))throw Error(JSON.stringify(state));
  const {data}=await page.send('Page.captureScreenshot',{captureBeyondViewport:true,clip:{x:0,y:0,width,height,scale:1}});
  writeFileSync(work+'/../shots/r4-page-'+width+'-'+mode+'.png',Buffer.from(data,'base64'));
  for(const sequence of [['gallery','devices','wall','space','type'],['updates-menu','progress','finished'],['machine-greeter','same-viewer-desktop']]){
    await page.eval(`document.querySelector('#${sequence[0]}-radio').focus()`);
    for(const name of sequence.slice(1)){
      await page.press('ArrowRight');
      if(!await page.eval(`document.querySelector('#${name}-radio').checked && document.querySelector('.panel img[src$="/${name}.webp"]').offsetWidth>0`))throw Error('keyboard '+name);
    }
  }
  if(mode==='on'){
    await page.eval("document.querySelector('#sharing-play').click()");
    await page.sleep(300);
    if(!await page.eval("!document.querySelector('#sharing-film').paused && document.querySelector('#sharing-still').hidden"))throw Error('sharing playback');
    await page.eval("document.querySelector('#sharing-play').click()");
    if(width===1440){
      const clip=await page.eval("(()=>{const r=document.querySelector('.lanes').getBoundingClientRect();return {x:r.x,y:r.y+scrollY,width:r.width,height:r.height,scale:1}})()");
      const shot=await page.send('Page.captureScreenshot',{captureBeyondViewport:true,clip});
      writeFileSync(work+'/../shots/r4-typing.png',Buffer.from(shot.data,'base64'));
    }
  }
  checks.push({width,mode,height,...state});console.log(JSON.stringify(checks.at(-1)));
 }
}finally{await close();writeFileSync(work+'/checks.json',JSON.stringify(checks,null,2))}
