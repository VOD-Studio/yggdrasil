import type { Mat, Rect } from './geometry';
import type { ResourceScope } from './resources';

export interface LightboxElements {
  overlay: HTMLDivElement;
  img: HTMLImageElement;
  caption: HTMLElement;
  counter: HTMLDivElement;
  errorBox: HTMLDivElement;
  toolbar: HTMLDivElement;
  badge: HTMLDivElement;
  downloadLink: HTMLAnchorElement;
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
  badgeTimer: number | undefined;
  toolbarHover: boolean;
  target?: Rect;
  baseW?: number;
  baseH?: number;
}

export interface LightboxActions {
  zoomBy: (factor: number, x: number, y: number, animate: boolean) => void;
  rotateCW: () => void;
  resetView: (animate: boolean) => void;
  close: (immediate: boolean) => void;
  gotoIndex: (index: number) => void;
}
