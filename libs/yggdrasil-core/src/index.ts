import type { ThemeName } from '@yggdrasil/shared';
import { initAnchorClick } from './anchor-click';
import { loadBrowserLibrary } from './browser-library';
import { scrollToHash } from './hash-scroll';
import { initMermaid } from './mermaid';
import { initPageReveal } from './page-entry';
import { initPostContent } from './post-content';
import { routeTransitions } from './route-transitions';
import { disposeShowcaseMasonry, initShowcaseMasonry } from './showcase-masonry';
import { applyResolvedTheme, startThemeTransition } from './theme-transition';
import { disposeTocSidebar, initTocSidebar } from './toc-sidebar';
import './style.css';

declare global {
  interface Window {
    __loadBrowserLibrary: typeof loadBrowserLibrary;
    __initPostContent: (selector: string) => void;
    __initMermaid: (selector: string, theme: ThemeName) => Promise<void>;
    __initAnchorClick: () => void;
    __scrollToHash: () => void;
    __startThemeTransition: (x: number, y: number) => void;
    __applyResolvedTheme: (isDark: boolean) => void;
    __initTocSidebar: typeof initTocSidebar;
    __disposeTocSidebar: typeof disposeTocSidebar;
    __routeTransitions: typeof routeTransitions;
    __initShowcaseMasonry: typeof initShowcaseMasonry;
    __disposeShowcaseMasonry: typeof disposeShowcaseMasonry;
  }
}

window.__initPostContent = initPostContent;
window.__loadBrowserLibrary = loadBrowserLibrary;
window.__initMermaid = initMermaid;
window.__initAnchorClick = initAnchorClick;
window.__scrollToHash = scrollToHash;
window.__startThemeTransition = startThemeTransition;
window.__applyResolvedTheme = applyResolvedTheme;
window.__initTocSidebar = initTocSidebar;
window.__disposeTocSidebar = disposeTocSidebar;
window.__routeTransitions = routeTransitions;
window.__initShowcaseMasonry = initShowcaseMasonry;
window.__disposeShowcaseMasonry = disposeShowcaseMasonry;

// Cross-document transitions can reveal before the WASM router connects.
initPageReveal();
