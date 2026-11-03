const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 800 } });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  const base = process.env.BASE || 'http://localhost:4173';
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ guideSeen: true })); });
  await p.goto(base);
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  // Plain-language mode is the default.
  console.log('package column hidden by default:', await p.locator('thead th', { hasText: 'Package' }).count() === 0);
  console.log('no package names in the table:', !(await p.locator('tbody').innerText()).includes('com.samsung.android.bixby.agent'));
  const row = p.locator('tbody tr', { hasText: 'Bixby Voice' });
  console.log('one-line reason shown:', (await row.innerText()).includes('Voice assistant.'));
  console.log('unrated apps say so:', (await p.locator('tbody tr', { hasText: 'Discord' }).innerText()).includes('No information about this app yet.'));
  // Search still finds apps by package name.
  await p.getByPlaceholder(/Search/).fill('bixby.agent');
  console.log('search by package still works:', await p.locator('tbody tr').count() === 1);
  await p.getByPlaceholder(/Search/).fill('');
  // Technical details on.
  await p.getByRole('button', { name: 'Settings' }).click();
  const dlg = p.getByRole('dialog', { name: 'Settings' });
  await dlg.getByRole('switch', { name: 'Show technical details' }).click();
  await dlg.getByRole('button', { name: 'Done' }).click();
  console.log('package column appears:', await p.locator('thead th', { hasText: 'Package' }).count() === 1 && (await p.locator('tbody').innerText()).includes('com.samsung.android.bixby.agent'));
  // Theme.
  const theme = () => p.evaluate(() => ({ attr: document.documentElement.dataset.theme || null, bg: getComputedStyle(document.body).backgroundColor }));
  await p.getByRole('button', { name: 'Settings' }).click();
  await dlg.getByLabel('Theme').selectOption('dark');
  let t = await theme(); console.log('dark theme applied:', t.attr === 'dark' && t.bg === 'rgb(21, 23, 26)');
  await dlg.getByLabel('Theme').selectOption('light');
  t = await theme(); console.log('light theme applied:', t.attr === 'light' && t.bg === 'rgb(255, 255, 255)');
  await dlg.getByLabel('Theme').selectOption('dark');
  await dlg.getByRole('button', { name: 'Done' }).click();
  await p.reload(); await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  t = await theme(); console.log('choices survive a reload:', t.attr === 'dark' && (await p.locator('thead th', { hasText: 'Package' }).count()) === 1);
  await p.getByRole('button', { name: 'Settings' }).click();
  await dlg.getByLabel('Theme').selectOption('system');
  console.log('system removes the override:', (await theme()).attr === null);
  // Plain mode also applies to the confirmation dialog.
  await dlg.getByRole('button', { name: 'Done' }).click();
  await p.getByRole('button', { name: 'Settings' }).click();
  await dlg.getByRole('switch', { name: 'Show technical details' }).click(); await dlg.getByRole('button', { name: 'Done' }).click();
  await p.locator('tbody tr', { hasText: 'Bixby Voice' }).click();
  await p.getByRole('button', { name: /^Disable \(/ }).click();
  const confirm = await p.getByRole('dialog').innerText();
  console.log('confirmation uses plain names:', confirm.includes('Bixby Voice') && !confirm.includes('com.samsung'));
  console.log('errors', errs); await b.close();
})();
