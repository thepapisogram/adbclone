const { chromium } = require('playwright');
(async () => {
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1200, height: 900 } });
  await p.addInitScript(() => { if (!localStorage.getItem('coolant.settings')) localStorage.setItem('coolant.settings', JSON.stringify({ plain: false, guideSeen: true })); });
  const errs = []; p.on('pageerror', e => errs.push(e.message)); p.on('console', m => m.type()==='error' && errs.push(m.text()));
  await p.goto(process.env.BASE || 'http://localhost:4173');
  await p.waitForFunction(() => document.querySelectorAll('tbody tr').length > 5);
  await p.getByRole('button', { name: 'Wake-ups', exact: true }).click();
  await p.locator('table[aria-label="Held wake locks"]').waitFor();
  console.log('held locks listed:', await p.locator('table[aria-label="Held wake locks"] tbody tr').count() >= 2);
  console.log('system locks summarised:', await p.getByText(/held by Android itself/).count() === 1);
  const wk = p.locator('table[aria-label="Wake-ups"] tbody tr');
  const nums = (await wk.locator('td:nth-child(2)').allTextContents()).map(Number);
  console.log('wake-ups sorted, most first:', nums.length >= 1 && nums.every((n, i) => i === 0 || nums[i - 1] >= n));
  await p.locator('table[aria-label="Held wake locks"] tbody tr').first().getByRole('button').click();
  await p.locator('ul[aria-label=Results] li').first().waitFor();
  console.log('restrict gives a result:', (await p.locator('ul[aria-label=Results] li').first().innerText()).startsWith('✓'));
  console.log('errors', errs); await b.close();
})();
