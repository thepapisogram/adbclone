const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 900 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  // details drawer
  await p.getByRole('button', { name: /^Details of Chrome/ }).first().click();
  const dlg = p.getByRole('dialog', { name: /^Details of/ });
  await dlg.waitFor();
  await dlg.getByLabel('Permissions').waitFor({ timeout: 10000 });
  const t = await dlg.innerText();
  console.log('shows the package and facts:', t.includes('com.android.chrome') && t.includes('Installed by'));
  console.log('permissions in plain words:', t.includes('Use the camera') && t.includes('Know your exact location'));
  console.log('says where the data came from:', t.includes('temporary helper'));
  console.log('rating and impact shown:', /if you remove it/i.test(t) && /what stops working/i.test(t));
  await p.keyboard.press('Escape');
  await dlg.waitFor({ state: 'detached' });
  console.log('Esc closes the drawer:', true);
  // check dialog
  await p.getByRole('button', { name: /^Names:/ }).click();
  const names = p.getByRole('dialog', { name: 'App names' });
  await names.waitFor();
  console.log('icon counts shown:', (await names.innerText()).includes('Icons:'));
  await names.getByRole('button', { name: 'Run the check' }).click();
  await names.getByLabel('Check result').waitFor({ timeout: 10000 });
  const r = await names.getByLabel('Check result').innerText();
  console.log('check lists every step:', ['Copy the helper', 'Start the helper', 'Read app names', 'Read icons'].every(x => r.includes(x)) && r.includes('Works'));
  console.log('errors', errs); await b.close();
})();
