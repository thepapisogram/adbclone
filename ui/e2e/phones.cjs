const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 900 } });
  await p.addInitScript(() => { localStorage.setItem('coolant.mock.second', '1'); if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  // apply to the second phone too
  const row = p.locator('tbody tr', { hasText: 'com.facebook.appmanager' }).first();
  await row.click();
  await p.getByRole('button', { name: /^Disable/ }).first().click();
  const dlg = p.getByRole('dialog');
  console.log('offers the other phone:', (await dlg.innerText()).includes('Galaxy S23'));
  await dlg.getByLabel(/Galaxy S23/).check();
  await dlg.getByRole('button', { name: 'Disable', exact: true }).click();
  await p.getByText(/Also on other phones: Galaxy S23/).waitFor({ timeout: 15000 });
  console.log('result reported per phone:', true);
  // phones tab
  await p.getByRole('button', { name: /^Phones/ }).click();
  await p.getByRole('region', { name: /Connected phones/ }).or(p.locator('[aria-label="Connected phones"]')).first().waitFor();
  console.log('cards for each ready phone:', await p.locator('.phonecard').count() >= 3);
  console.log('unauthorized phone explained:', await p.getByText(/Waiting for you to allow this computer/).count() === 1);
  await p.getByLabel('Phone Galaxy S23 (mock)').getByRole('button', { name: 'Mirror screen' }).click();
  await p.getByText('Opening the mirror window…').waitFor();
  console.log('mirror starts:', true);
  await p.getByLabel('Phone Galaxy S23 (mock)').getByRole('button', { name: 'Use this phone' }).click();
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  console.log('use this phone switches to apps:', await p.getByRole('button', { name: 'Apps', exact: true }).evaluate(e => e.classList.contains('active')));
  console.log('errors', errs); await b.close();
})();
