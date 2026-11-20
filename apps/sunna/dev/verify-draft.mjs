// Exercise edits inside the debounce window using the normal sheet handlers.
import assert from 'node:assert/strict';
import { launch } from './cdp.mjs';

const { page, close } = await launch({ port: 9471, scale: 1 });
try {
  for (const field of ['address', 'key']) {
    await page.goto((process.env.SUNNA_MOCK_URL || 'http://127.0.0.1:8766/ui/') + '?scene=empty');
    await page.click('#add-button');
    await page.eval(`{
      window.__sunnaMock.commands.check_machine = () => new Promise(resolve => window.answer = resolve);
      window.__sunnaMock.commands.add_machine = ({draft}) => { window.saved = draft; throw Error('fixture'); };
      window.edit = (name,value) => {
        const el = document.querySelector('.sheet input[name=' + name + ']');
        el.value = value;
        el.dispatchEvent(new Event('input'));
      };
      edit('address','first.local');
    }`);
    await page.sleep(600);
    const clean = await page.eval(`(async () => {
      edit('${field}', '${field === 'address' ? 'second.local' : 'fixture'}');
      answer({state:'ready', identity:'A'.repeat(43)});
      await new Promise(resolve => setTimeout(resolve,0));
      document.querySelector('.sheet form').requestSubmit();
      await new Promise(resolve => setTimeout(resolve,0));
      return saved.identity === null && saved.about === null;
    })()`);
    assert.ok(clean, 'stale probe survived ' + field + ' edit');
  }
} finally { close(); }
