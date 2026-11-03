// Renders the brand assets to brand/out with Chromium (Playwright). Run: NODE_PATH=$(npm root -g) node brand/build.cjs
const { chromium } = require('playwright');
const fs = require('fs'); const path = require('path');
const dir = __dirname; const out = path.join(dir, 'out');
const icon = fs.readFileSync(path.join(dir, 'icon.svg'), 'utf8').replace(/<\?xml[^>]*>/, '');
const inner = (svg) => svg.replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '');
const iconDataUri = 'data:image/svg+xml;base64,' + Buffer.from(icon).toString('base64');
const font = "Inter Display, Inter, 'Segoe UI', Arial, sans-serif";

// Full-bleed square version for avatars (platforms crop it to a circle themselves).
const avatar = icon.replace(/rx="232"/g, 'rx="0"');

const page = (w, h, body) => `<!doctype html><meta charset="utf-8"><style>
*{box-sizing:border-box;margin:0} body{width:${w}px;height:${h}px;overflow:hidden;font-family:${font};color:#fff;background:#070B1A}
</style>${body}`;

const header = page(1500, 500, `
<div style="position:relative;width:1500px;height:500px;background:
  radial-gradient(700px 400px at 12% 10%, rgba(45,212,191,.28), transparent 60%),
  radial-gradient(800px 500px at 95% 90%, rgba(99,102,241,.35), transparent 60%), #070B1A">
  <div style="position:absolute;left:560px;top:96px;display:flex;align-items:center;gap:34px">
    <img src="${iconDataUri}" width="170" height="170">
    <div>
      <div style="font-size:112px;font-weight:800;letter-spacing:-3px;line-height:1">Coolant</div>
      <div style="font-size:38px;font-weight:600;color:#7DD3FC;margin-top:10px">Debloat. Cool down. Speed up.</div>
    </div>
  </div>
  <div style="position:absolute;left:560px;top:336px;font-size:28px;white-space:nowrap;color:#CBD5E1;line-height:1.35">
    Android cleaner for your computer. No root needed. A safety snapshot before every removal.
  </div>
</div>`);

const social = (shot) => page(1200, 630, `
<div style="position:relative;width:1200px;height:630px;background:
  radial-gradient(600px 400px at 8% 0%, rgba(45,212,191,.30), transparent 60%),
  radial-gradient(700px 500px at 100% 100%, rgba(99,102,241,.38), transparent 60%), #070B1A">
  <div style="position:absolute;left:64px;top:60px;display:flex;align-items:center;gap:22px">
    <img src="${iconDataUri}" width="104" height="104">
    <div style="font-size:76px;font-weight:800;letter-spacing:-2px">Coolant</div>
  </div>
  <div style="position:absolute;left:64px;top:218px;width:470px;font-size:52px;font-weight:800;line-height:1.08;letter-spacing:-1px">
    A cooler, cleaner, faster Android.</div>
  <ul style="position:absolute;left:64px;top:402px;width:480px;font-size:28px;color:#CBD5E1;line-height:1.55;list-style:none;padding:0">
    <li>✓ Remove bloat with safety snapshots</li><li>✓ Watch heat and calm busy apps</li><li>✓ Works over USB or Wi-Fi, no root</li></ul>
  ${shot ? `<img src="${shot}" style="position:absolute;left:575px;top:70px;width:585px;border-radius:18px;box-shadow:0 30px 80px rgba(0,0,0,.55);border:1px solid rgba(255,255,255,.12)">
  <div style="position:absolute;left:575px;top:560px;font-size:18px;color:#94A3B8">Screenshot with sample data</div>` : ''}
</div>`);

(async () => {
  const b = await chromium.launch();
  const snap = async (html, w, h, file, scale = 1) => {
    const p = await b.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: scale });
    await p.setContent(html); await p.waitForTimeout(300);
    await p.screenshot({ path: path.join(out, file) }); await p.close();
  };
  fs.writeFileSync(path.join(out, 'icon.svg'), icon);
  fs.writeFileSync(path.join(out, 'avatar.svg'), avatar);
  await snap(`<!doctype html><body style="margin:0;background:transparent">${icon.replace('width="1024" height="1024"', 'width="1024" height="1024" style="display:block"')}`, 1024, 1024, 'icon-1024.png');
  await snap(`<body style="margin:0">${avatar.replace('width="1024" height="1024"', 'width="400" height="400" style="display:block"')}`, 400, 400, 'avatar-400.png');
  await snap(header, 1500, 500, 'x-header-1500x500.png');
  const shotFile = process.argv[2];
  const shot = shotFile && fs.existsSync(shotFile) ? 'data:image/png;base64,' + fs.readFileSync(shotFile).toString('base64') : null;
  await snap(social(shot), 1200, 630, 'social-card-1200x630.png');
  await b.close();
  console.log('brand assets written to', out);
})();
