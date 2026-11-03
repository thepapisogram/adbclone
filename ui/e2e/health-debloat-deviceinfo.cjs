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
  const SAFE_HIGH = ['com.facebook.appmanager', 'com.facebook.services', 'com.facebook.system', 'com.netflix.partner.activation', 'com.android.egg', 'com.samsung.android.app.tips'];
  console.log('SAFE rows:', n.length, '| all ticked:', (await checked()) === n.length);
  console.log('safe preset holds exactly the confident safe apps:', SAFE_HIGH.every(x => n.includes(x)) && n.length === SAFE_HIGH.length);
  console.log('a less-sure safe app is not in Safe:', !n.includes('com.vendor.analyticsservice'));
  console.log('safe excludes advanced/unsafe/unrated/protected:', !['com.android.chrome', 'com.google.android.gms', 'com.vendor.unknownservice', 'com.android.systemui', 'com.google.android.gm'].some(x => n.includes(x)));
  console.log('summary:', (await p.locator('.summary').innerText()).replace(/\s+/g, ' ').slice(0, 85));
  await p.getByRole('radio', { name: 'Balanced' }).click();
  n = await names();
  console.log('BALANCED rows:', n.length, '| ticked is still only the safe ones:', (await checked()) === SAFE_HIGH.length);
  console.log('the less-sure app moved to Balanced:', n.includes('com.vendor.analyticsservice') && n.includes('com.google.android.gm'));
  console.log('Gmail is not pre-ticked:', await p.locator('tr', { hasText: 'com.google.android.gm' }).locator('input').isChecked() === false);
  console.log('what you would lose is shown:', (await p.locator('.debloat').innerText()).includes('You lose this Google app. You can reinstall it from the Play Store.'));
  console.log('the less-sure rating is marked:', await p.locator('tr', { hasText: 'com.vendor.analyticsservice' }).getByText('less sure').count() === 1);
  console.log('Chrome is never offered, it is the default browser:', !n.includes('com.android.chrome'));
  const held = p.locator('details.heldback');
  await held.locator('summary').click();
  console.log('held-back list names the reason:', (await held.innerText()).includes('Chrome') && (await held.innerText()).includes('your browser'));
  await p.getByRole('radio', { name: 'Aggressive' }).click();
  n = await names();
  console.log('AGGRESSIVE rows:', n.length, '| ticked:', await checked(), '| unrated shown:', n.includes('com.vendor.unknownservice'), '| Play Store shown:', n.includes('com.android.vending'));
  console.log('unsafe and protected hidden:', !n.some(x => ['com.google.android.gms','com.android.systemui','com.android.phone','com.android.chrome'].includes(x)));
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
