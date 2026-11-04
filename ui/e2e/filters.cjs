const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1300, height: 800 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  await p.waitForFunction(() => /Names: \d+ from the phone/.test(document.body.innerText), null, { timeout: 20000 });
  const rows = () => p.locator('tbody tr').count();
  const all = await rows();
  await p.locator('details.morefilters summary').click();
  // Category
  const cat = p.getByLabel('Category');
  console.log('category filter lists the kinds present:', (await cat.locator('option').allTextContents()).some(t => t.startsWith('partner helper')));
  await cat.selectOption('partner-stub');
  console.log('category narrows the list:', await rows() === 4 && (await p.locator('tbody').innerText()).includes('Facebook'));
  await cat.selectOption('any');
  // Installer needs the helper's details to have arrived.
  const inst = p.getByLabel('Installed by');
  await p.waitForFunction(() => !document.querySelector('select[aria-label="Installed by"]').disabled, null, { timeout: 15000 });
  await inst.selectOption('play');
  const play = await rows();
  console.log('installed-by Google Play shows only user apps:', play > 0 && play < all && !(await p.locator('tbody').innerText()).includes('Facebook App Manager'));
  await inst.selectOption('preinstalled');
  console.log('installed-by "came with the phone" shows system apps:', (await p.locator('tbody').innerText()).includes('Facebook App Manager'));
  await inst.selectOption('any');
  // Duplicates
  await p.getByLabel('Duplicates only').check();
  const t = await p.locator('tbody').innerText();
  console.log('duplicates only: the vendor gallery and browser:', await rows() === 2 && t.includes('Samsung Gallery') && t.includes('Samsung Internet'));
  console.log('says which app it duplicates:', t.includes('Does the same job as Photos') && t.includes('Does the same job as Chrome'));
  await p.getByLabel('Duplicates only').uncheck();
  // Protected default app note
  await p.getByPlaceholder(/Search/).fill('Chrome');
  console.log('a default app says so:', (await p.locator('tbody').innerText()).includes('You use this as your browser.'));
  console.log('errors', errs); await b.close();
})();
