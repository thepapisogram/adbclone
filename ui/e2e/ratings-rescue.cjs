const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 900 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  // rescue
  await p.getByRole('button', { name: 'Snapshots', exact: true }).click();
  const rescue = p.locator('details.rescue');
  await rescue.waitFor();
  await rescue.locator('summary').click();
  const t = await rescue.innerText();
  console.log('rescue shows safe mode steps:', t.includes('Samsung') && t.includes('Reboot to safe mode'));
  console.log('rescue offers newest snapshot:', /newest snapshot is/.test(t) || /no snapshot exists yet/.test(t));
  // ratings
  await p.getByRole('button', { name: 'Ratings', exact: true }).click();
  await p.getByRole('region', { name: 'Rate an app' }).waitFor();
  await p.getByLabel('App to rate').selectOption({ index: 2 });
  await p.getByLabel('Note').fill('my note');
  await p.getByRole('button', { name: 'Save rating' }).click();
  await p.getByText('Saved your rating.').waitFor();
  console.log('own rating listed:', await p.locator('table[aria-label="Your ratings"] tbody tr').count() === 1);
  await p.getByRole('button', { name: 'Remove' }).first().click();
  await p.waitForFunction(() => !document.querySelector('table[aria-label="Your ratings"]'));
  console.log('own rating removed:', true);
  const [chooser] = await Promise.all([p.waitForEvent('filechooser'), p.getByRole('button', { name: 'Import a ratings file…' }).click()]);
  await chooser.setFiles({ name: 'friend.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify([{ id: 'com.x.y', risk: 'recommended' }, { id: 'com.android.systemui', risk: 'recommended' }, { id: 'bad id', risk: 'advanced' }])) });
  await p.getByText(/Imported 1 rating from friend\.json \(2 skipped/).waitFor();
  console.log('import keeps only usable ratings:', true);
  await p.getByRole('button', { name: 'Remove' }).first().click();
  await p.waitForFunction(() => !document.querySelector('table[aria-label="Imported files"]'));
  const [c2] = await Promise.all([p.waitForEvent('filechooser'), p.getByRole('button', { name: 'Import a ratings file…' }).click()]);
  await c2.setFiles({ name: 'junk.json', mimeType: 'application/json', buffer: Buffer.from('nope') });
  await p.getByRole('alert').waitFor();
  console.log('junk file rejected:', (await p.getByRole('alert').innerText()).includes('not a Coolant ratings file'));
  console.log('errors', errs); await b.close();
})();
