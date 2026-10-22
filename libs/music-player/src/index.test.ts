import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  player: {
    pause: vi.fn(),
    play: vi.fn(),
    setQueue: vi.fn(),
    setSource: vi.fn(),
    destroy: vi.fn(),
  },
  getPlayer: vi.fn(),
  createKiteSource: vi.fn((url: string) => ({ url })),
}));

vi.mock('@violetjs/kite-player/element', () => ({
  getPlayer: mocks.getPlayer,
  createKiteSource: mocks.createKiteSource,
}));

let create: typeof import('./index').create;
let instance: ReturnType<typeof create>;
const config = { apiUrl: 'https://music.example.com', playlists: ['123', '456'] };

beforeEach(async () => {
  vi.resetModules();
  vi.clearAllMocks();
  mocks.getPlayer.mockReturnValue(mocks.player);
  document.body.innerHTML = '<div id="player"></div><div id="other"></div>';
  document.documentElement.classList.remove('dark');
  ({ create } = await import('./index'));
});

afterEach(() => {
  instance?.destroy();
  instance = null;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  document.body.innerHTML = '';
});

describe('layout music player', () => {
  it('does not initialize kite when the layout has already unmounted', () => {
    expect(create('missing', config)).toBeNull();
    expect(mocks.getPlayer).not.toHaveBeenCalled();
  });

  it('configures the source and attributes before connecting the custom element', () => {
    const container = document.getElementById('player')!;
    const append = container.append.bind(container);
    const spy = vi.spyOn(container, 'append').mockImplementation((...nodes) => {
      const element = nodes[0] as Element;
      expect(element.getAttribute('api')).toBe(config.apiUrl);
      expect(element.getAttribute('playlists')).toBe('123,456');
      expect(element.getAttribute('theme')).toBe('light');
      expect(mocks.getPlayer).toHaveBeenCalled();
      append(...nodes);
    });
    instance = create('player', config);
    expect(instance).not.toBeNull();
    expect(spy).toHaveBeenCalledOnce();
    expect(mocks.getPlayer).toHaveBeenCalledWith({
      source: { url: config.apiUrl },
      storageKey: `yggdrasil-music-player:${config.apiUrl}`,
    });
    expect(mocks.player.play).not.toHaveBeenCalled();
  });

  it('keeps the same view and playback core when the layout renders again', () => {
    instance = create('player', config);
    expect(create('player', config)).toBe(instance);
    expect(document.querySelectorAll('kite-player')).toHaveLength(1);
    expect(mocks.getPlayer).toHaveBeenCalledOnce();
    expect(mocks.player.pause).not.toHaveBeenCalled();
  });

  it('follows the actual site theme without recreating the player', async () => {
    instance = create('player', config);
    const element = document.querySelector('kite-player')!;
    document.documentElement.classList.add('dark');
    await vi.waitFor(() => expect(element.getAttribute('theme')).toBe('dark'));
    document.documentElement.classList.remove('dark');
    await vi.waitFor(() => expect(element.getAttribute('theme')).toBe('light'));
    expect(document.querySelector('kite-player')).toBe(element);
    expect(mocks.getPlayer).toHaveBeenCalledOnce();
    expect(mocks.player.pause).not.toHaveBeenCalled();
  });

  it('pauses on frontend exit, detaches theme observation and preserves the reusable core', async () => {
    instance = create('player', config);
    const element = document.querySelector('kite-player')!;
    instance!.destroy();
    instance!.destroy();
    expect(document.querySelector('kite-player')).toBeNull();
    expect(mocks.player.pause).toHaveBeenCalledOnce();
    expect(mocks.player.destroy).not.toHaveBeenCalled();
    document.documentElement.classList.add('dark');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(element.getAttribute('theme')).toBe('light');

    instance = create('player', config);
    expect(document.querySelector('kite-player')?.getAttribute('theme')).toBe('dark');
    expect(mocks.player.setQueue).not.toHaveBeenCalled();
    expect(mocks.player.play).not.toHaveBeenCalled();
  });

  it('replaces the old layout without letting a stale handle pause the replacement', () => {
    const old = create('player', config)!;
    instance = create('other', config);
    expect(document.querySelectorAll('kite-player')).toHaveLength(1);
    expect(document.getElementById('player')?.childElementCount).toBe(0);
    expect(mocks.player.pause).toHaveBeenCalledOnce();
    old.destroy();
    expect(mocks.player.pause).toHaveBeenCalledOnce();
    expect(document.getElementById('other')?.querySelector('kite-player')).not.toBeNull();
  });

  it('clears the old queue when changing API services', () => {
    instance = create('player', config);
    instance!.destroy();
    instance = create('player', { apiUrl: 'https://other.example.com', playlists: ['789'] });
    expect(mocks.player.setQueue).toHaveBeenCalledWith([], 0, null, false);
    expect(mocks.player.setSource).toHaveBeenCalledWith({ url: 'https://other.example.com' });
  });

  it('does not mount with an empty playlist list', () => {
    expect(create('player', { ...config, playlists: [] })).toBeNull();
    expect(mocks.getPlayer).not.toHaveBeenCalled();
  });

  it.each(['javascript:alert(1)', 'https://user:secret@music.example.com'])(
    'rejects unsafe service URL %s',
    (apiUrl) => {
      expect(() => create('player', { ...config, apiUrl })).toThrow();
      expect(mocks.getPlayer).not.toHaveBeenCalled();
    },
  );

  it('rejects mixed-content audio service configuration', () => {
    vi.stubGlobal('location', { protocol: 'https:' });
    expect(() => create('player', { ...config, apiUrl: 'http://music.example.com' })).toThrow(
      'HTTPS 页面需要使用 HTTPS 音乐服务。',
    );
    expect(mocks.getPlayer).not.toHaveBeenCalled();
  });
});
