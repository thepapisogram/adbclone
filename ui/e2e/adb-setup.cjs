const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch();
  const base = process.env.BASE || 'http://localhost:4173';
  const errs = [];
  const open = async (query) => {
    const p = await browser.newPage({ viewport: { width: 1200, height: 800 } });
    await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
    p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
    await p.goto(base + query);
    await p.waitForSelector('.devicecard, .error');
    return p;
  };
  // adb present: no banner.
  let p = await open('');
  console.log('no banner when adb is present:', await p.getByText(/was not found/).count() === 0);
  await p.close();
  // adb missing: banner, download, ready.
  p = await open('?noadb=1');
  console.log('banner when adb is missing:', await p.getByText(/was not found/).count() === 1);
  await p.getByRole('button', { name: 'Set it up now' }).click();
  const dlg = p.getByRole('dialog', { name: 'Set up adb' });
  console.log('explains what adb is and where it comes from:', (await dlg.innerText()).includes('dl.google.com'));
  await dlg.getByRole('button', { name: 'Download adb from Google' }).click();
  await dlg.getByRole('progressbar').waitFor();
  await dlg.getByText(/MB of/).waitFor({ timeout: 10000 });
  console.log('progress is shown:', true);
  await dlg.getByText(/adb is ready/).waitFor({ timeout: 15000 });
  console.log('ready after the download:', true);
  await p.waitForFunction(() => !document.body.innerText.includes('was not found'));
  console.log('banner goes away:', true);
  await p.close();
  // Failure, then retry.
  p = await open('?noadb=1&adbfail=1');
  await p.getByRole('button', { name: 'Set it up now' }).click();
  const d2 = p.getByRole('dialog', { name: 'Set up adb' });
  await d2.getByRole('button', { name: 'Download adb from Google' }).click();
  await d2.getByRole('alert').waitFor({ timeout: 15000 });
  const msg = await d2.getByRole('alert').innerText();
  console.log('failure explains itself and offers a manual route:', msg.includes('network is unreachable') && msg.includes('PATH'));
  await d2.getByRole('button', { name: 'Try again' }).click();
  await d2.getByText(/adb is ready/).waitFor({ timeout: 15000 });
  console.log('retry works:', true);
  console.log('errors', errs); await browser.close();
})();
