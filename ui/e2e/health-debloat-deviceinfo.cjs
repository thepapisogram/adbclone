const { chromium } = require('playwright');
const [SHOT1, SHOT2] = process.argv.slice(2);
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1250, height: 800 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto((process.env.BASE || 'http://localhost:4173'));
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  await p.waitForSelector('.devicecard');
  console.log('device card:', (await p.locator('.devicecard').innerText()).slice(0, 110));

  // ---- Temperature
  await p.getByRole('button', { name: 'Temperature' }).click();
  await p.selectOption('select[aria-label="Refresh interval"]', '1');
  await p.waitForFunction(() => document.querySelectorAll('svg.spark path').length === 2, null, { timeout: 15000 });
  console.log('charts drawn: 2, cpu bars:', await p.locator('.cpubar').count(), ', sensor rows:', await p.locator('table.sensors tr').count());
  console.log('badge:', await p.locator('.actions .pill').first().innerText());
  console.log('battery card:', (await p.locator('.card').first().innerText()).replace(/\s+/g, ' ').slice(0, 90));
  console.log('chart aria:', (await p.locator('svg.spark').first().getAttribute('aria-label')).slice(0, 70));
  await p.screenshot({ path: SHOT1 });
  await p.getByRole('button', { name: 'See which apps are using the CPU' }).click();
  console.log('link goes to Running tab:', await p.locator('.running').count() === 1);

  // ---- Debloat wizard
  await p.getByRole('button', { name: 'Debloat' }).click();
  await p.waitForFunction(() => !document.body.innerText.includes('Measuring'));
  const names = async () => p.locator('.debloat tbody tr td:nth-child(2) .mono').allTextContents();
  const checked = async () => p.locator('.debloat tbody tr input:checked').count();
  let n = await names();
  console.log('SAFE rows:', n.length, '| ticked:', await checked());
  console.log('safe excludes advanced/unsafe/unrated:', !['com.android.chrome', 'com.google.android.gms', 'com.vendor.unknownservice', 'com.android.systemui'].some(x => n.includes(x)));
  console.log('summary:', (await p.locator('.summary').innerText()).replace(/\s+/g, ' ').slice(0, 85));
  await p.getByRole('radio', { name: 'Balanced' }).click();
  n = await names();
  console.log('BALANCED rows:', n.length, '| ticked:', await checked(), '| chrome shown:', n.includes('com.android.chrome'), '| chrome unticked:', await p.locator('tr', { hasText: 'com.android.chrome' }).locator('input').isChecked() === false);
  await p.getByRole('radio', { name: 'Aggressive' }).click();
  n = await names();
  console.log('AGGRESSIVE rows:', n.length, '| ticked:', await checked(), '| unrated shown:', n.includes('com.vendor.unknownservice'), '| unsafe hidden:', !n.some(x => ['com.google.android.gms','com.android.systemui','com.android.phone'].includes(x)));
  await p.screenshot({ path: SHOT2 });
  await p.getByRole('radio', { name: 'Safe' }).click();
  const before = (await names()).length;
  await p.getByRole('button', { name: /^Disable \d+ app/ }).click();
  await p.getByRole('dialog').getByRole('button', { name: 'Disable' }).click();
  await p.waitForSelector('.notice'); console.log('notice:', (await p.locator('.notice').innerText()).slice(0, 60));
  await p.waitForFunction((b) => document.querySelectorAll('.debloat tbody tr').length < b, before);
  console.log('wizard list after apply:', (await names()).length, '(was', before + ')');
  // restore from snapshot
  await p.getByRole('button', { name: 'Open Snapshots' }).click();
  await p.locator('tbody tr', { hasText: 'Before disable' }).getByRole('button', { name: 'Restore…' }).click();
  await p.getByRole('dialog', { name: 'Restore snapshot' }).getByRole('button', { name: /^Restore \d+ change/ }).click();
  await p.getByText(/now matches the snapshot/).waitFor();
  await p.getByRole('dialog').getByRole('button', { name: 'Close' }).click();
  await p.getByRole('button', { name: 'Debloat' }).click();
  await p.waitForFunction((b) => document.querySelectorAll('.debloat tbody tr').length === b, before);
  console.log('wizard list after restore:', (await names()).length);
  console.log('errors', errs); await b.close();
})();
