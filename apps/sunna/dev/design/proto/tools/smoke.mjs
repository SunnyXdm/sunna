import {launch} from './cdp.mjs';
import {mkdirSync,writeFileSync} from 'node:fs';
const dir=new URL('../shots/',import.meta.url).pathname;
const {page,close}=await launch({width:1280,height:800,scale:1,port:9335});
try {
for(const [name,file,q,w,h] of [['desktop','desktop','',1280,800],['overview','desktop','view=overview',900,600],['menu','desktop','view=menu',900,600],['phone','phone','',390,844],['phone-menu','phone','view=menu',390,844],['form','phone','view=add',390,844]]){
 await page.send('Emulation.setDeviceMetricsOverride',{width:w,height:h,deviceScaleFactor:1,mobile:false});
 await page.goto(`http://100.108.56.3:8790/${file}.html?${q}&test=0&still=1`);await page.sleep(100);await page.shot(dir+'smoke-'+name+'.png');console.log(name,await page.eval('({cur,sel,body:document.body.scrollWidth,app:app.clientWidth})'));
}console.log(page.logs);writeFileSync(dir+'smoke-console.json',JSON.stringify(page.logs,null,2));
}finally{close();}
