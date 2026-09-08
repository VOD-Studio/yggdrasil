import type { ThemeName } from '@yggdrasil/shared';
import { initAnchorClick } from './anchor-click';
import { scrollToHash } from './hash-scroll';
import { initMermaid } from './mermaid';
import { initPostContent } from './post-content';
import { routeTransitions } from './route-transitions';
import { applyResolvedTheme, startThemeTransition } from './theme-transition';
import { initTocSidebar } from './toc-sidebar';
import './style.css';

declare global {
  interface Window {
    __initPostContent: (selector: string) => void;
    __initMermaid: (selector: string, theme: ThemeName) => Promise<void>;
    __initAnchorClick: () => void;
    __scrollToHash: () => void;
    __startThemeTransition: (x: number, y: number) => void;
    __applyResolvedTheme: (isDark: boolean) => void;
    __initTocSidebar: () => void;
    __routeTransitions: typeof routeTransitions;
  }
}

window.__initPostContent = initPostContent;
window.__initMermaid = initMermaid;
window.__initAnchorClick = initAnchorClick;
window.__scrollToHash = scrollToHash;
window.__startThemeTransition = startThemeTransition;
window.__applyResolvedTheme = applyResolvedTheme;
window.__initTocSidebar = initTocSidebar;
window.__routeTransitions = routeTransitions;
