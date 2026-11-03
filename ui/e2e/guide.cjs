const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch();
  const p = await browser.newPage({ viewport: { width: 1200, height: 900 } });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  const base = process.env.BASE || 'http://localhost:4173';
  await p.goto(base);                       // a brand-new visitor: no stored settings
  const dlg = p.getByRole('dialog', { name: 'Getting started' });
  await dlg.waitFor();
  console.log('guide opens by itself the first time:', true);
  await p.waitForFunction(() => /phone is connected/.test(document.querySelector('[aria-label=Status]').innerText), null, { timeout: 10000 });
  const status = await dlg.getByLabel('Status').innerText();
  console.log('live checks: adb ready and a phone seen:', /adb .* is ready/.test(status) && /phone is connected/.test(status));
  console.log('an unauthorised phone is called out:', /has not allowed this computer/.test(status));
  await dlg.getByLabel('Phone maker').waitFor();
  await p.waitForFunction(() => document.querySelector('select[aria-label="Phone maker"]').value === 'google', null, { timeout: 5000 });
  console.log('maker is picked from the connected phone:', true);
  await dlg.getByLabel('Phone maker').selectOption('xiaomi');
  console.log('Xiaomi notes mention the security toggle:', (await dlg.getByLabel('Xiaomi / Redmi / POCO notes').innerText()).includes('USB debugging (Security settings)'));
  await dlg.getByLabel('Phone maker').selectOption('samsung');
  console.log('Samsung notes mention Auto Blocker:', (await dlg.getByLabel('Samsung notes').innerText()).includes('Auto Blocker'));
  console.log('build number location is maker-specific:', (await dlg.innerText()).includes('Software information > Build number'));
  await dlg.getByRole('button', { name: 'Done' }).click();
  await p.waitForFunction(() => !document.querySelector('.modal'));
  await p.reload(); await p.waitForSelector('.devicecard');
  console.log('not shown again after Done:', await p.getByRole('dialog', { name: 'Getting started' }).count() === 0);
  await p.getByRole('button', { name: 'Help getting started' }).click();
  console.log('the ? button reopens it:', await p.getByRole('dialog', { name: 'Getting started' }).count() === 1);
  await p.getByRole('dialog', { name: 'Getting started' }).getByRole('button', { name: 'Open Connect…' }).click();
  console.log('can jump to the Wi-Fi connect dialog:', await p.getByRole('dialog', { name: 'Connect device' }).count() === 1);
  // With no adb the guide itself offers the download.
  const p2 = await browser.newPage({ viewport: { width: 1200, height: 900 } });
  await p2.goto(base + '?noadb=1');
  const d2 = p2.getByRole('dialog', { name: 'Getting started' });
  await d2.waitFor();
  console.log('guide offers the adb download when adb is missing:', await d2.getByRole('button', { name: 'Download adb from Google' }).count() === 1);
  console.log('errors', errs); await browser.close();
})();
