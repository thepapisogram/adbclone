const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 900 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  await p.getByRole('button', { name: 'Install', exact: true }).click();
  console.log('drop zone explains what it takes:', (await p.getByLabel('Drop apps here').innerText()).includes('.xapk'));
  await p.getByRole('button', { name: 'Choose files…' }).click();
  await p.waitForFunction(() => document.querySelectorAll('.installtable tr').length === 4);
  const table = await p.locator('.installtable').innerText();
  console.log('apps are described (name, kind, size, version):', table.includes('Hello Notes') && table.includes('app file') && table.includes('17.6 MB') && table.includes('version 2.4.1'));
  console.log('a bundle with game data warns about it:', table.includes('game data (OBB files)'));
  console.log('an unsupported file is explained:', table.includes('only .apk, .apks, .xapk and .apkm'));
  // Untick Replace so the "already exists" file fails with advice.
  await p.getByLabel('Replace if already installed').uncheck();
  await p.getByRole('button', { name: /^Install \d+ on/ }).click();
  const dlg = p.getByRole('dialog', { name: 'Confirm install' });
  console.log('confirmation names the phone and the permissions:', (await dlg.innerText()).includes('Pixel 7') && (await dlg.innerText()).includes('7 permissions'));
  console.log('confirmation warns to trust the source:', (await dlg.innerText()).includes('sources you trust'));
  await dlg.getByRole('button', { name: 'Install' }).click();
  await p.waitForFunction(() => document.querySelectorAll('.installtable .bad-mark').length >= 2, null, { timeout: 15000 });
  const after = await p.locator('.installtable').innerText();
  console.log('successes and failures are shown per app:', after.includes('Hello Notes installed.') && after.includes('Replace if already installed'));
  // Retry only the failed one after ticking Replace: remove finished, keep failed ones? Failed items cannot be retried automatically; re-add.
  await p.getByLabel('Replace if already installed').check();
  // Backup
  await p.getByLabel('Search apps to back up').fill('');
  console.log('backup lists user apps, not preinstalled ones, by default:', await p.getByLabel('Back up WhatsApp').count() === 1 && await p.getByLabel('Back up Facebook App Manager').count() === 0);
  await p.getByLabel('Include apps that came with the phone').check();
  console.log('preinstalled apps can be included:', await p.getByLabel('Back up Facebook App Manager').count() === 1);
  await p.getByLabel('Back up WhatsApp').check(); await p.getByLabel('Back up Firefox').check(); await p.getByLabel('Back up Discord').check();
  await p.getByRole('button', { name: /^Back up 3 apps/ }).click();
  await p.getByText(/Done: \d+ saved/).waitFor({ timeout: 15000 });
  const summary = await p.getByRole('status').filter({ hasText: 'Done:' }).innerText();
  console.log('summary counts saved and failed:', summary.includes('2 saved') && summary.includes('1 could not be saved'));
  const res = await p.getByLabel('Backup results').innerText();
  console.log('a failure says why:', res.includes('not installed for you right now'));
  console.log('splits are reported:', /\d+ files?, \d+(\.\d)? MB/.test(res));
  console.log('errors', errs); await b.close();
})();
