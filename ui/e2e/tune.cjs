const { chromium } = require('playwright');
const SHOT = process.argv[2];
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 900 } });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto((process.env.BASE || 'http://localhost:4173'));
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  await p.getByRole('button', { name: 'Tune', exact: true }).click();
  await p.waitForFunction(() => document.querySelectorAll('.tune tbody tr:not(:has(.empty))').length > 3);
  // quick toggle on/off
  const anim = p.getByRole('switch', { name: /Calmer animations/ });
  await anim.click(); await p.waitForSelector('.notice');
  console.log('toggle on -> banner:', (await p.locator('.notice').innerText()).slice(0, 55), '| checked:', await anim.isChecked());
  await anim.click(); await p.waitForFunction(() => !document.querySelector('.notice'));
  console.log('toggle off -> banner gone, switch is off:', !(await anim.isChecked()));
  // offenders: protected (unsafe) apps are not listed; default picks are user apps only
  const listed = await p.locator('.tune tbody tr td:nth-child(2) .mono').allTextContents();
  console.log('offenders listed:', listed.length, '| unsafe hidden:', !listed.some(x => ['com.google.android.gms','com.android.systemui','com.android.phone'].includes(x)));
  await p.waitForFunction(() => document.querySelectorAll('.tune tbody tr.sel').length > 0, null, { timeout: 15000 });
  const ticked = await p.locator('.tune tbody tr.sel td:nth-child(2) .mono').allTextContents();
  console.log('default ticked (user apps):', ticked.length, ticked.slice(0, 3));
  // pick a system app too, enable "stop now", start cool-down
  await p.locator('.tune tbody tr', { hasText: 'com.android.chrome' }).click();
  await p.getByLabel(/Also stop the ticked apps/).click();
  await p.getByRole('button', { name: /^Start cool-down/ }).click();
  const dlg = p.getByRole('dialog', { name: 'Confirm cool-down' });
  console.log('confirm lists:', (await dlg.locator('li').allTextContents()).join(' | ').slice(0, 150));
  await p.screenshot({ path: SHOT });
  await dlg.getByRole('button', { name: 'Start' }).click();
  await p.waitForFunction(() => [...document.querySelectorAll('ul[aria-label=Results] li')].some(l => l.textContent.includes('Stop ')), null, { timeout: 15000 });
  const res = await p.locator('ul[aria-label=Results] li').allTextContents();
  console.log('results:', res.length, 'steps, failures:', res.filter(x => x.startsWith('✗')).length);
  console.log('before panel:', (await p.locator('.beforeafter p').first().innerText()).slice(0, 70));
  await p.getByRole('button', { name: 'Check temperature again' }).click();
  await p.waitForFunction(() => document.querySelectorAll('.beforeafter p').length === 2);
  console.log('after panel:', (await p.locator('.beforeafter p').nth(1).innerText()).slice(0, 80));
  console.log('restricted section listed:', await p.locator('h3', { hasText: 'Apps with restricted background' }).count() === 1);
  // allow one app again
  const n0 = await p.locator('.block:has(h3:text("Apps with restricted")) .saved').count();
  await p.locator('.block:has(h3:text("Apps with restricted")) .saved').first().getByRole('button', { name: 'Allow again' }).click();
  await p.waitForFunction((n) => document.querySelectorAll('.block .saved').length === n - 1 || n === 1, n0);
  console.log('allow again reduces restricted list from', n0);
  // undo all
  await p.getByRole('button', { name: 'Undo all changes' }).click();
  await p.waitForFunction(() => !document.querySelector('.notice'));
  console.log('undo all -> banner gone; switches off:', !(await p.getByRole('switch', { name: /Battery saver/ }).isChecked()), '| restricted section gone:', await p.locator('h3', { hasText: 'Apps with restricted background' }).count() === 0);
  // caches
  await p.getByRole('button', { name: 'Clear all app caches' }).click();
  await p.getByText(/Caches cleared/).waitFor(); console.log('caches cleared message shown');
  console.log('errors', errs); await b.close();
})();
