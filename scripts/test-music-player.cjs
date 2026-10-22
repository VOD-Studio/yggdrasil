#!/usr/bin/env bun
// Build first: bun run --cwd libs/music-player build
// This exercises the real bundle with a local public API fixture, never a live music account.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');

const root = path.resolve(__dirname, '..');
const bundle = fs.readFileSync(path.join(root, 'public/music-player/player.js'), 'utf8');
const origin = 'https://music-player.test';
const cover = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256"><rect width="256" height="256" fill="#40a02b"/></svg>')}`;
const track = {
  id: '1', title: '播放器回归曲目', artists: [{ id: '2', name: '本地测试' }],
  album: { id: '3', name: '回归歌单', cover }, durationMs: 45000,
  access: 'free', playable: true, stream: '/tone.wav', lyrics: '/lyrics',
};
const playlist = { id: '123', name: '本地歌单', cover, trackCount: 1, tracks: [track] };
const lyrics = {
  lines: [{ time: 0, duration: 45000, text: '本地歌词', translation: '只用于回归测试' }],
  chorus: [{ start: 1000, end: 10000 }], instrumental: false,
};
const audio = Buffer.alloc(44 + 8000 * 2 * 45);
audio.write('RIFF', 0); audio.writeUInt32LE(audio.length - 8, 4); audio.write('WAVEfmt ', 8);
audio.writeUInt32LE(16, 16); audio.writeUInt16LE(1, 20); audio.writeUInt16LE(1, 22);
audio.writeUInt32LE(8000, 24); audio.writeUInt32LE(16000, 28);
audio.writeUInt16LE(2, 32); audio.writeUInt16LE(16, 34); audio.write('data', 36);
audio.writeUInt32LE(audio.length - 44, 40);

const fixture = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>
:root { --color-paper-accent:#40a02b; --color-paper-primary:#4c4f69; --color-paper-secondary:#5c5f77; --color-paper-entry:#e6e9ef; --color-paper-theme:#eff1f5; }
html.dark { --color-paper-primary:#cdd6f4; --color-paper-secondary:#bac2de; --color-paper-entry:#181825; --color-paper-theme:#1e1e2e; }
body { margin:0; min-height:2000px; background:var(--color-paper-theme); color:var(--color-paper-primary); font-family:system-ui,sans-serif; }
main { padding:32px; } button { padding:8px; }
</style></head><body><main id="page"><h1>第一页</h1><button id="navigate">前台 SPA 切页</button></main><div id="player"></div>
<script src="/music-player/player.js"></script><script>
window.playerConfig = { apiUrl:location.origin, playlists:['123'] };
window.playerHandle = YggdrasilMusicPlayer.create('player', playerConfig);
window.originalPlayerElement = document.querySelector('kite-player');
document.getElementById('navigate').onclick = () => {
  history.pushState(null, '', '/next');
  document.getElementById('page').innerHTML = '<h1>第二页</h1>';
};
</script></body></html>`;

async function setup(context, requests) {
  await context.route('**/*', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    assert.equal(url.origin, origin, 'fixture must not send external requests');
    assert.equal(request.headers().authorization, undefined, 'no management credentials');
    requests.push(url.pathname);
    const json = (body) => route.fulfill({ json: body });
    switch (url.pathname) {
      case '/': case '/next':
        return route.fulfill({ contentType: 'text/html', body: fixture });
      case '/music-player/player.js':
        return route.fulfill({ contentType: 'text/javascript', body: bundle });
      case '/player/v1/info':
        return json({ version: 'fixture', login: 'none', quality: 'standard' });
      case '/player/v1/playlists/123':
        return json(playlist);
      case '/player/v1/tracks/1':
        return json(track);
      case '/lyrics':
        return json(lyrics);
      case '/tone.wav': {
        const range = request.headers().range?.match(/^bytes=(\d+)-(\d*)$/);
        const start = range ? Number(range[1]) : 0;
        const end = range?.[2] ? Math.min(Number(range[2]), audio.length - 1) : audio.length - 1;
        const headers = { 'Accept-Ranges': 'bytes' };
        if (range) headers['Content-Range'] = `bytes ${start}-${end}/${audio.length}`;
        return route.fulfill({
          status: range ? 206 : 200, contentType: 'audio/wav', headers,
          body: audio.subarray(start, end + 1),
        });
      }
      default:
        throw new Error(`Unexpected request: ${url.pathname}`);
    }
  });
}

(async () => {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {}),
  });
  try {
    for (const mobile of [false, true]) {
      for (const reduced of [false, true]) {
        const name = `${mobile ? 'mobile' : 'desktop'}-${reduced ? 'reduced' : 'motion'}`;
        const context = await browser.newContext({
          viewport: mobile ? { width: 390, height: 844 } : { width: 1440, height: 1000 },
          reducedMotion: reduced ? 'reduce' : 'no-preference', colorScheme: 'light',
        });
        try {
          const requests = [];
          await setup(context, requests);
          const page = await context.newPage();
          const errors = [];
          page.on('pageerror', (error) => errors.push(error.message));
          await page.goto(origin);
          await page.waitForFunction(() => window.__kitePlayer?.state.current?.id === '1');
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.playing), false);
          assert.equal(requests.includes('/tone.wav'), false, 'initial load must not autoplay');
          await page.getByRole('button', { name: '播放', exact: true }).click();
          await page.waitForFunction(() => window.__kitePlayer.state.playing && window.__kitePlayer.state.time > 0.1);
          await page.locator('#navigate').click();
          await page.getByRole('heading', { name: '第二页' }).waitFor();
          assert.equal(await page.evaluate(() => document.querySelector('kite-player') === window.originalPlayerElement), true);
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.playing), true);
          const loads = requests.filter((p) => p === '/player/v1/playlists/123').length;
          await page.evaluate(() => document.documentElement.classList.add('dark'));
          await page.waitForFunction(() => document.querySelector('kite-player').getAttribute('theme') === 'dark');
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.playing), true);
          assert.equal(requests.filter((p) => p === '/player/v1/playlists/123').length, loads);
          await page.evaluate(() => document.querySelector('kite-player').open('lyrics'));
          await page.getByRole('dialog', { name: '正在播放', exact: true }).waitFor();
          await page.evaluate(async () => {
            const elements = document.querySelector('kite-player').shadowRoot.querySelectorAll('*');
            const animations = [...elements].flatMap((element) => element.getAnimations())
              .filter((animation) => animation.effect.getTiming().iterations !== Infinity);
            await Promise.all(animations.map((animation) => animation.finished.catch(() => {})));
          });
          if (mobile) assert.equal(await page.evaluate(() => document.documentElement.style.overflow), 'hidden');
          if (process.env.MUSIC_PLAYER_ARTIFACT_DIR) {
            fs.mkdirSync(process.env.MUSIC_PLAYER_ARTIFACT_DIR, { recursive: true });
            await page.screenshot({ path: path.join(process.env.MUSIC_PLAYER_ARTIFACT_DIR, `${name}.png`) });
          }
          await page.evaluate(() => window.playerHandle.destroy());
          assert.equal(await page.locator('kite-player').count(), 0);
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.playing), false);
          assert.equal(await page.evaluate(() => document.documentElement.style.overflow), '');
          await page.evaluate(() => {
            window.playerHandle = YggdrasilMusicPlayer.create('player', window.playerConfig);
          });
          await page.locator('kite-player[theme="dark"]').waitFor({ state: 'attached' });
          await page.getByRole('button', { name: '播放', exact: true }).waitFor();
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.playing), false);
          await page.evaluate(() => {
            window.__kitePlayer.seek(4);
            window.__kitePlayer.setMode('sequence'); // Structural change flushes the snapshot immediately.
          });
          await page.reload();
          await page.waitForFunction(() => window.__kitePlayer?.state.current?.id === '1');
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.playing), false);
          assert.equal(await page.evaluate(() => window.__kitePlayer.state.mode), 'sequence');
          assert.ok(await page.evaluate(() => window.__kitePlayer.state.time >= 3), 'reload restores playback position');
          assert.deepEqual(errors, [], `${name}: browser errors`);
          console.log(`PASS ${name}: no autoplay, playback, SPA, theme, teardown, re-entry, reload`);
        } finally {
          await context.close();
        }
      }
    }
  } finally {
    await browser.close();
  }
})().catch((error) => { console.error(error); process.exitCode = 1; });
