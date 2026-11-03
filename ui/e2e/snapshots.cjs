const { chromium } = require('playwright');
const SHOT = process.argv[2];
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1150, height: 780 }, acceptDownloads: true });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto((process.env.BASE || 'http://localhost:4173'));
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  const counts = async () => (await p.locator('.actions span').first().innerText());
  console.log('start:', (await counts()).match(/\d+ disabled/)[0]);
  // Disable photos (advanced) + all recommended.
  await p.selectOption('select[aria-label=Risk]', 'recommended');
  await p.locator('thead input[type=checkbox]').check();
  await p.selectOption('select[aria-label=Risk]', 'any');
  await p.locator('tbody tr', { hasText: 'com.google.android.apps.photos' }).click(); // add Photos to the selection
  await p.getByRole('button', { name: /^Disable \(/ }).click();
  await p.getByRole('dialog').getByRole('button', { name: 'Disable' }).click();
  await p.waitForSelector('.notice');
  console.log('notice:', (await p.locator('.notice').innerText()).slice(0, 55));
  await p.selectOption('select[aria-label=Risk]', 'any');
  await p.waitForFunction(() => /11 disabled/.test(document.body.innerText));
  const disabledAfter = (await counts()).match(/(\d+) disabled/)[1];
  console.log('disabled after batch:', disabledAfter);
  // Snapshots tab: one auto snapshot; take a manual one of the debloated state.
  await p.getByRole('button', { name: 'Snapshots', exact: true }).click();
  console.log('auto snapshots listed:', await p.locator('tbody tr .pill', { hasText: 'auto' }).count());
  await p.getByPlaceholder('Name this snapshot').fill('Debloated'); await p.getByRole('button', { name: 'Take snapshot now' }).click();
  await p.waitForSelector('.status.ok'); console.log('manual saved ->', (await p.locator('.status.ok').innerText()).slice(0, 40));
  // Restore the auto one (before debloat).
  await p.locator('tbody tr', { hasText: 'Before disable' }).getByRole('button', { name: 'Restore…' }).click();
  const dlg = p.getByRole('dialog', { name: 'Restore snapshot' });
  await dlg.getByText(/change/).first().waitFor();
  console.log('plan:', (await dlg.locator('p').first().innerText()).replace(/\s+/g,' ').slice(0, 80));
  await p.screenshot({ path: SHOT });
  await dlg.getByRole('button', { name: /^Restore \d+ change/ }).click();
  await dlg.getByText(/now matches the snapshot/).waitFor();
  console.log('restore result:', (await dlg.locator('p').first().innerText()).replace(/\s+/g,' '));
  await dlg.getByRole('button', { name: 'Close' }).click();
  await p.getByRole('button', { name: 'Apps' }).click(); await p.waitForTimeout(600);
  console.log('disabled after restore:', (await counts()).match(/(\d+) disabled/)[1]);
  await p.getByRole('button', { name: 'Snapshots', exact: true }).click();
  console.log('undo snapshot created:', await p.locator('tbody tr', { hasText: 'Before restoring' }).count() === 1);
  // Restore "Debloated" -> photos fails in the mock.
  await p.locator('tbody tr', { hasText: 'Debloated' }).last().getByRole('button', { name: 'Restore…' }).click();
  await dlg.getByRole('button', { name: /^Restore \d+ change/ }).click();
  await dlg.getByText(/still differ/).waitFor();
  console.log('partial:', (await dlg.locator('p').first().innerText()).replace(/\s+/g,' '), '|', (await dlg.locator('ul.changes li').first().innerText()).slice(0, 70));
  await dlg.getByRole('button', { name: 'Close' }).click();
  // Export + delete
  const [dl] = await Promise.all([p.waitForEvent('download'), p.locator('tbody tr', { hasText: 'Debloated' }).last().getByRole('button', { name: 'Export' }).click()]);
  console.log('export file:', dl.suggestedFilename());
  const row = p.locator('tbody tr', { hasText: 'Debloated' }).last();
  await row.getByRole('button', { name: 'Delete' }).click(); await row.getByRole('button', { name: 'Really delete' }).click();
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length === 3);
  console.log('deleted: true');
  console.log('errors', errs); await b.close();
})();
