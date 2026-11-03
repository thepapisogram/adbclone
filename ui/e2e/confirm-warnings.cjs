const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 800 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  // Plain-language mode is on: the warnings use names, not package ids.
  for (const name of ['Chrome', 'Gmail', 'Unknownservice']) await p.locator('tbody tr', { hasText: name }).first().click();
  await p.getByRole('button', { name: /^Disable \(/ }).click();
  const text = await p.getByRole('dialog').innerText();
  console.log('warns that Chrome is the default browser:', /Chrome[\s\S]*This is your browser/.test(text));
  console.log('shows what Gmail removal costs:', text.includes('You lose this Google app'));
  console.log('says an unrated app is unrated:', text.includes('Nobody has rated this app yet'));
  console.log('no package ids in plain mode:', !text.includes('com.android.chrome') && !text.includes('com.google.android.gm'));
  await p.getByRole('dialog').getByRole('button', { name: 'Cancel' }).click();
  // Enabling something again carries no removal warnings.
  console.log('errors', errs); await b.close();
})();
