// Use the app's CDP driver with its scratch files retained for inspection.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {homedir} from 'node:os';
export const work=homedir()+'/ext/work/site/r4';
mkdirSync(work+'/tmp',{recursive:true});
process.env.TMPDIR=work+'/tmp';
const source=readFileSync(new URL('../../apps/sunna/dev/cdp.mjs',import.meta.url),'utf8')
  .replace(/^[ \t]*rmSync\([^\n]+$/gm,'')
  .replace('format: "jpeg", quality: 88','format: "png"')
  .replace('const page = {','const page = { on,')
  .replaceAll('.jpg','.png').replace('"-crf", "20"','"-crf", "0"')
  .replace('const dir = `${tmpdir()}/sunna-cdp-frames-${Date.now()}`','const dir = path + ".frames-" + Date.now()');
writeFileSync(work+'/driver.mjs',source);
export const {launch}=await import(work+'/driver.mjs');
