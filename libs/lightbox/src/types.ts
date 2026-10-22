import type { Inset, Mat, Rect } from './geometry';
import type { ResourceScope } from './resources';

export interface LightboxElements {
  overlay: HTMLDivElement;
  img: HTMLImageElement;
  caption: HTMLElement;
  counter: HTMLSpanElement;
  errorBox: HTMLDivElement;
  // 顶栏（计数 / 工具组 / 关闭）与底栏（图注 / 缩略条 / 键盘提示）：决定适配区域，
  // 放大后空闲时一起淡出。
  topbar: HTMLDivElement;
  bottom: HTMLDivElement;
  // 缩放与旋转工具组；错误态隐藏。
  toolbar: HTMLDivElement;
  zoomOutBtn: HTMLButtonElement;
  zoomInBtn: HTMLButtonElement;
  zoomLevel: HTMLButtonElement;
  downloadLink: HTMLAnchorElement;
  thumbStrip: HTMLDivElement | null;
  thumbs: HTMLButtonElement[];
  prevBtn: HTMLButtonElement | null;
  nextBtn: HTMLButtonElement | null;
}

export type GestureState =
  | { mode: 'pan'; pointerId: number; startX: number; startY: number; startUser: Mat }
  | {
      mode: 'pinch';
      idA: number;
      idB: number;
      startDist: number;
      startMid: { x: number; y: number };
      startScale: number;
      startUser: Mat;
    }
  | { mode: 'dragclose'; pointerId: number; startY: number; dy: number };

export interface LightboxState extends LightboxElements {
  originNode: HTMLElement;
  gallery: HTMLElement[];
  index: number | null;
  isSingle: boolean;
  openScrollY: number;
  scrollClose: { rect: Rect; transform: string; imgOpacity: number; overlayOpacity: number } | null;
  origSrc: string;
  altText: string;
  closing: boolean;
  scope: ResourceScope;
  reduced: boolean;
  // 查看器操控：deg 累计顺时针角（0/90/180/…不取模，CSS 插值方向正确；
  // 几何用 normDeg 归一）；scale 相对 fit 的倍率；user 矩阵
  // 承载缩放锚点与平移，null = 恒等（飞行/图集切换路径仍写 transformFor
  // 字符串，首次操控时才经 ensureNormalized 归一到列表基态）。
  view: { deg: number; scale: number; user: Mat | null };
  normalized: boolean;
  pointers: Map<number, { x: number; y: number }>;
  gesture: GestureState | null;
  idleTimer: number | undefined;
  chromeHover: boolean;
  // 适配区域边距：打开、换图与视口变化时按顶栏 / 底栏实测重算，其余路径只读缓存。
  inset: Inset;
  target?: Rect;
  baseW?: number;
  baseH?: number;
}

export interface LightboxActions {
  zoomBy: (factor: number, x: number, y: number, animate: boolean) => void;
  stepZoom: (dir: 1 | -1) => void;
  rotate: (dir: 1 | -1) => void;
  resetView: (animate: boolean) => void;
  close: (immediate: boolean) => void;
  gotoIndex: (index: number) => void;
}
