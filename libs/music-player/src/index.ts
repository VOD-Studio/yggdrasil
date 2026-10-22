import { createKiteSource, getPlayer } from '@violetjs/kite-player/element';

export interface MusicPlayerConfig {
  apiUrl: string;
  playlists: string[];
}

export interface MusicPlayerInstance {
  destroy(): void;
}

let active: { container: HTMLElement; instance: MusicPlayerInstance } | undefined;
let sourceUrl: string | undefined;

/** The layout owns the view; kite owns one shared playback core for the document. */
export function create(containerId: string, config: MusicPlayerConfig): MusicPlayerInstance | null {
  const container = document.getElementById(containerId);
  if (!container?.isConnected) return null;
  if (active?.container === container) return active.instance;

  const url = new URL(config.apiUrl);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) {
    throw new Error('音乐服务必须使用不含凭据的 HTTP(S) 地址。');
  }
  if (location.protocol === 'https:' && url.protocol !== 'https:') {
    throw new Error('HTTPS 页面需要使用 HTTPS 音乐服务。');
  }
  if (config.playlists.length === 0) return null;

  active?.instance.destroy();
  const player = getPlayer({
    source: createKiteSource(config.apiUrl),
    storageKey: `yggdrasil-music-player:${config.apiUrl}`,
  });
  if (sourceUrl && sourceUrl !== config.apiUrl) {
    player.pause();
    player.setQueue([], 0, null, false);
    player.setSource(createKiteSource(config.apiUrl));
  }
  sourceUrl = config.apiUrl;

  const element = document.createElement('kite-player');
  element.setAttribute('api', config.apiUrl);
  element.setAttribute('playlists', config.playlists.join(','));
  element.setAttribute('collapse', 'scroll');
  for (const [name, value] of Object.entries({
    '--kite-accent': 'var(--color-paper-accent)',
    '--kite-fg': 'var(--color-paper-primary)',
    '--kite-muted': 'var(--color-paper-secondary)',
    '--kite-card': 'var(--color-paper-entry)',
    '--kite-sheet': 'var(--color-paper-theme)',
    '--kite-font': 'var(--font-sans, system-ui, sans-serif)',
  })) {
    element.style.setProperty(name, value);
  }
  const root = document.documentElement;
  const syncTheme = () => {
    element.setAttribute('theme', root.classList.contains('dark') ? 'dark' : 'light');
  };
  syncTheme();
  const observer = new MutationObserver(syncTheme);
  observer.observe(root, { attributes: true, attributeFilter: ['class'] });
  container.append(element);

  let destroyed = false;
  const instance: MusicPlayerInstance = {
    destroy() {
      if (destroyed) return;
      destroyed = true;
      observer.disconnect();
      element.remove();
      // Leaving the frontend pauses playback. Do not destroy kite's global core:
      // getPlayer() would otherwise return that destroyed singleton on re-entry.
      if (active?.instance === instance) {
        player.pause();
        active = undefined;
      }
    },
  };
  active = { container, instance };
  return instance;
}
