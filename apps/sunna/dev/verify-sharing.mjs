// Run serve.py on a private port, then set SUNNA_MOCK_URL to its loopback URL.
import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {launch} from './cdp.mjs';

const base = process.env.SUNNA_MOCK_URL || 'http://127.0.0.1:49061';
const out = new URL('./design/shots-app/', import.meta.url).pathname;
const temp = mkdtempSync(tmpdir()+'/sunna-share-shots-');
const {page, close} = await launch({port:9661, scale:1});
const modes = ['off','installing','starting','needs-permission','on','busy','error','system','unsupported','stopping'];
const expected = ['Off','Installing','Starting','Waiting for permission','Sharing','is connected','needs system packages','administrator','Wayland','Stopping…'];
try {
  for (const [width,height] of [[980,660],[680,480]]) {
    await page.send('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});
    for (const view of ['gallery','space']) {
      const captures = [];
      for (const [i,mode] of modes.entries()) {
        await page.goto(base+'/ui/?count=1&view='+view+'&sharing='+mode);
        await page.click('#settings-button');
        for(let retry=0; retry<30; retry++){
          if(await page.eval(`document.querySelector('.this-sharing-text')?.textContent.includes(${JSON.stringify(expected[i])})`)) break;
          await page.sleep(100);
        }
        assert.ok(await page.eval(`document.querySelector('.this-sharing-text')?.textContent.includes(${JSON.stringify(expected[i])})`),mode);
        await page.eval("document.querySelector('.this-computer').scrollIntoView({block:'start'})");
        await page.sleep(350);
        const file=temp+'/'+mode+'.png';
        await page.shot(file); captures.push(file);
        if(mode==='error' || mode==='on'){
          await page.eval("Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>window.copied=text}})");
          await page.click(mode==='error'?'.share-actions button':'.copy-link');
          assert.ok(await page.eval(mode==='error'
            ? "window.copied === 'sudo apt install xvfb x11-utils dbus-x11 xfce4'"
            : "window.copied === window.__sunnaMock.commands.sharing_status().link"));
          if(mode==='on'){
            await page.click('.show-qr');
            assert.ok(await page.eval("!!document.querySelector('.qr-card svg')"));
          }
        }
        if(mode==='busy'){
          await page.click('#share-host');
          assert.ok(await page.eval("document.querySelector('.this-sharing-text').textContent.includes('Stop sharing anyway?')"));
          await page.eval("document.querySelector('.share-actions button:last-child').click()");
          await page.sleep(400);
          assert.equal(await page.eval("document.querySelector('#share-host').checked"),false);
        }
        if(mode==='off'){
          await page.click('#share-host');await page.sleep(600);
          assert.ok(await page.eval("document.querySelector('#share-host').checked && !document.querySelector('.link-row').hidden"));
        }
      }
      execFileSync('montage',[...captures,'-tile','3x4','-geometry','+0+0',out+'sharing-'+view+'-'+width+'.png']);
    }
    const views=[];
    for(const view of ['devices','wall','space','type','gallery']){
      await page.goto(base+'/ui/?count=1&view='+view+'&state=not-sharing');
      await page.sleep(700);
      assert.ok(await page.eval("!!document.querySelector('[data-state=not-sharing]') && document.querySelector('#computer-host').innerText.includes('signed in')"),view);
      const file=temp+'/'+view+'.png';await page.shot(file);views.push(file);
    }
    execFileSync('montage',[...views,'-tile','2x3','-geometry','+0+0',out+'sharing-probe-'+width+'.png']);
  }
  const emit = patch => page.eval(`window.__sunnaMock.emit('sharing-changed', {...window.__sunnaMock.commands.sharing_status(), ...${JSON.stringify(patch)}})`);
  const checked = () => page.eval("document.querySelector('#share-host').checked");
  const action = () => page.eval("document.querySelector('.share-actions code')?.textContent || ''");
  await page.goto(base+'/ui/?count=1&sharing=on');
  await page.click('#settings-button');
  await page.sleep(350);
  await emit({state:'stopping'});
  assert.ok(await checked());
  await emit({state:'error', fix:{label:'Try again.',command:'arbitrary helper text'}});
  assert.ok(await checked());
  assert.equal(await action(), '');
  await emit({state:'off', starts_by_itself:true, managed_by:'cli'});
  assert.equal(await checked(),false);
  assert.ok(await page.eval("document.querySelector('.this-sharing-text').textContent.startsWith('Off until you next log in')"));
  await emit({state:'off', starts_by_itself:true, managed_by:'system'});
  assert.ok(await page.eval("document.querySelector('.this-sharing-text').textContent.startsWith('Off until the next boot')"));
  assert.equal(await action(),'sudo systemctl disable --now sunna-host.service');
  await emit({state:'error',managed_by:'cli'});
  assert.equal(await checked(),false);
  await emit({state:'off',starts_by_itself:false,managed_by:'system'});
  assert.equal(await action(),'sudo systemctl enable --now sunna-host.service');
  assert.ok(!page.logs.some(l=>l.includes('EXCEPTION')), 'JavaScript exception');
  console.log('Passed 40 Settings captures, on/off and viewer confirmation, and five probe views.');
} finally {await close();rmSync(temp,{recursive:true,force:true});}
