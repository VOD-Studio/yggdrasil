import {
  clampScale,
  DOUBLE_CLICK_SCALE,
  DRAG_CLOSE_PX,
  fitCentered,
  matIdentity,
  matMultiply,
  matScale,
  matToCss,
  matTranslate,
  panBy,
  SCROLL_CLOSE_PX,
  scrollFlightTransform,
  transformFor,
  wheelZoomFactor,
} from './geometry';
import { ResourceScope } from './resources';
import type { LightboxActions, LightboxState } from './types';
import {
  applyView,
  baseGeometry,
  ensureNormalized,
  isErrorShown,
  measureInset,
  pokeToolbar,
  rectOf,
  resetView,
  rotate,
  stepZoom,
  zoomBy,
} from './view';

// 模态对话框：Tab 在灯箱内可见、可用的控件间循环，不落回背后的页面。
function trapFocus(overlay: HTMLElement, ev: KeyboardEvent): void {
  const items = [...overlay.querySelectorAll<HTMLElement>('button:not(:disabled), a[href]')].filter(
    (el) => el.checkVisibility?.({ checkOpacity: true, checkVisibilityCSS: true }) ?? true,
  );
  if (items.length === 0) return;
  const first = items[0];
  const last = items[items.length - 1];
  const current = document.activeElement;
  const inside = current instanceof HTMLElement && items.includes(current);
  if (ev.shiftKey && (!inside || current === first)) {
    ev.preventDefault();
    last.focus();
  } else if (!ev.shiftKey && (!inside || current === last)) {
    ev.preventDefault();
    first.focus();
  }
}

const CHROME_SELECTOR = '.lightbox-topbar, .lightbox-bottom, .lightbox-nav';

function onScrollClose(s: LightboxState, actions: LightboxActions): void {
  if (s.closing) return;
  const progress = Math.min(Math.abs(window.scrollY - s.openScrollY) / SCROLL_CLOSE_PX, 1);
  if (progress === 0 && !s.scrollClose) return;
  if (s.reduced || !s.target || isErrorShown(s)) {
    actions.close(false);
    return;
  }
  if (!s.scrollClose) {
    const style = getComputedStyle(s.img);
    s.scrollClose = {
      rect: rectOf(s.img),
      transform: style.transform,
      imgOpacity: Number(style.opacity),
      overlayOpacity: Number(getComputedStyle(s.overlay).opacity),
    };
    // 先提交同构首帧；快照包含实际绘制中的缩放/旋转，不依赖动画目标值。
    s.img.style.transition = 'none';
    s.img.style.transform = scrollFlightTransform(
      s.scrollClose.rect,
      s.scrollClose.rect,
      0,
      s.scrollClose.transform,
    );
    void s.img.offsetHeight;
  }
  if (progress >= 1) {
    actions.close(false);
    return;
  }
  // 短过渡平滑离散的鼠标滚轮步进；连续滚动持续更新原图实时位置。
  s.img.style.transition = 'transform 100ms ease-out, opacity 100ms ease-out';
  s.img.style.transform = scrollFlightTransform(
    s.scrollClose.rect,
    rectOf(s.originNode),
    progress,
    s.scrollClose.transform,
  );
  s.img.style.opacity = String(s.scrollClose.imgOpacity * (1 - progress));
  s.overlay.style.transition = 'opacity 100ms ease-out';
  s.overlay.style.opacity = String(s.scrollClose.overlayOpacity * (1 - progress));
}

export function bindInteractions(s: LightboxState, actions: LightboxActions): () => void {
  const scope = new ResourceScope();

  // 点背景关闭（点图片本身不关，因箭头在图上、避免误关）
  scope.listen(s.overlay, 'click', (ev: MouseEvent): void => {
    if (ev.target === s.overlay) actions.close(false);
  });

  // 触屏仍由图片指针手势处理，避免同时触发页面滚动关闭。缩略条例外：
  // 它横向滚动（touch-action: pan-x、overscroll-behavior: contain，不会带动页面）。
  scope.listen(
    s.overlay,
    'touchmove',
    (ev: TouchEvent): void => {
      if (ev.target instanceof Element && ev.target.closest('.lightbox-thumbs')) return;
      ev.preventDefault();
    },
    { passive: false },
  );

  scope.listen(window, 'scroll', () => onScrollClose(s, actions), { passive: true });

  // 普通滚轮保留页面滚动，驱动图片缩回原位；Ctrl/⌘ + 滚轮及触控板
  // 捏合（ctrlKey）继续锚定光标缩放。顶栏、底栏与箭头上阻止滚动，避免误关；
  // 缩略条把竖向滚轮折成横向滚动。
  scope.listen(
    s.overlay,
    'wheel',
    (ev: WheelEvent): void => {
      if (s.closing) return;
      const chrome = ev.target instanceof Element ? ev.target.closest(CHROME_SELECTOR) : null;
      if (chrome) {
        ev.preventDefault();
        if (s.thumbStrip && chrome === s.bottom && !ev.ctrlKey) {
          s.thumbStrip.scrollLeft +=
            Math.abs(ev.deltaX) > Math.abs(ev.deltaY) ? ev.deltaX : ev.deltaY;
        }
        return;
      }
      if (!ev.ctrlKey && !ev.metaKey) return;
      ev.preventDefault();
      zoomBy(s, wheelZoomFactor(ev.deltaY, ev.deltaMode), ev.clientX, ev.clientY, false);
    },
    { passive: false },
  );

  // 双击：fit ↔ 2.5× 切换（缩小态先回 fit），锚定点击点
  scope.listen(s.img, 'dblclick', (ev: MouseEvent): void => {
    if (!s?.target || isErrorShown(s)) return;
    ev.preventDefault();
    const goal = Math.abs(s.view.scale - 1) > 1e-3 ? 1 : DOUBLE_CLICK_SCALE;
    zoomBy(s, goal / s.view.scale, ev.clientX, ev.clientY, true);
  });

  // 键盘：Esc 关；图集 ←→ 切换；+/- 按档缩放；R / Shift+R 左右旋转；0 重置。
  // 带 Ctrl/⌘/Alt 的组合键留给浏览器（如 ⌘R 刷新、Ctrl+加号缩放页面）。
  const keyHandler = (ev: KeyboardEvent): void => {
    if (ev.key === 'Escape') {
      actions.close(false);
      return;
    }
    if (ev.key === 'Tab') {
      trapFocus(s.overlay, ev);
      return;
    }
    if (ev.ctrlKey || ev.metaKey || ev.altKey) return;
    if (!s.isSingle && s.gallery.length > 1) {
      if (ev.key === 'ArrowLeft') {
        ev.preventDefault();
        actions.gotoIndex((s.index ?? 0) - 1);
        return;
      }
      if (ev.key === 'ArrowRight') {
        ev.preventDefault();
        actions.gotoIndex((s.index ?? 0) + 1);
        return;
      }
    }
    if (ev.key === '+' || ev.key === '=') {
      ev.preventDefault();
      stepZoom(s, 1);
    } else if (ev.key === '-' || ev.key === '_') {
      ev.preventDefault();
      stepZoom(s, -1);
    } else if (ev.key === 'r' || ev.key === 'R') {
      ev.preventDefault();
      rotate(s, ev.shiftKey ? -1 : 1);
    } else if (ev.key === '0') {
      ev.preventDefault();
      resetView(s, true);
    }
  };
  scope.listen(document, 'keydown', keyHandler);

  // 视口尺寸变化：重算 fit。已操控态保持 deg/scale 由 applyView 重锚定；
  // 未操控态（transformFor 路径）按原逻辑重算居中目标。
  const resizeHandler = (): void => {
    const st = s;
    if (!st?.target || st.closing) return;
    st.inset = measureInset(st);
    if (st.normalized) {
      applyView(s, false);
      return;
    }
    const natW = st.img.naturalWidth || 1;
    const natH = st.img.naturalHeight || 1;
    const target = fitCentered(natW, natH, window.innerWidth, window.innerHeight, st.inset);
    st.target = target;
    st.img.style.transition = 'none';
    st.img.style.transform = transformFor(target, st.baseW ?? target.w, st.baseH ?? target.h);
  };
  scope.listen(window, 'resize', resizeHandler);

  // ---- 指针手势（鼠标/触控统一 Pointer Events；img CSS 为 touch-action:none）----
  scope.listen(s.img, 'dragstart', (ev: Event): void => {
    ev.preventDefault();
  });
  scope.listen(s.img, 'pointerdown', (ev: PointerEvent) => onPointerDown(s, ev));
  scope.listen(s.img, 'pointermove', (ev: PointerEvent) => onPointerMove(s, ev));
  scope.listen(s.img, 'pointerup', (ev: PointerEvent) => onPointerUp(s, actions, ev));
  scope.listen(s.img, 'pointercancel', (ev: PointerEvent) => onPointerUp(s, actions, ev));

  // 工具栏空闲自动隐藏：overlay 上任何指针活动唤醒
  scope.listen(s.overlay, 'pointermove', (): void => {
    pokeToolbar(s);
  });
  scope.listen(s.overlay, 'pointerdown', (): void => {
    pokeToolbar(s);
  });

  // 图集导航箭头点击（stopPropagation 防止冒泡到 overlay 触发关闭）
  if (s.prevBtn)
    scope.listen(s.prevBtn, 'click', (ev: MouseEvent): void => {
      ev.stopPropagation();
      actions.gotoIndex((s.index ?? 0) - 1);
    });
  if (s.nextBtn)
    scope.listen(s.nextBtn, 'click', (ev: MouseEvent): void => {
      ev.stopPropagation();
      actions.gotoIndex((s.index ?? 0) + 1);
    });
  // 指针停在栏上时不淡出。顶栏、底栏本身不接收指针，只看可交互的子元素。
  for (const el of [
    s.toolbar,
    s.topbar.querySelector('.lightbox-close'),
    s.thumbStrip,
    s.prevBtn,
    s.nextBtn,
  ]) {
    if (!el) continue;
    scope.listen(el, 'pointerenter', () => {
      s.chromeHover = true;
      pokeToolbar(s);
    });
    scope.listen(el, 'pointerleave', () => {
      s.chromeHover = false;
      pokeToolbar(s);
    });
  }
  return () => {
    scope.dispose();
    s.scope.clearTimeout(s.idleTimer);
    s.pointers.clear();
    s.gesture = null;
  };
}

// ---- 指针手势处理 ----

function onPointerDown(s: LightboxState, ev: PointerEvent): void {
  if (!s?.target || s.closing || isErrorShown(s)) return;
  if (ev.pointerType === 'mouse' && ev.button !== 0) return; // 仅主键
  s.img.setPointerCapture(ev.pointerId);
  s.pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  pokeToolbar(s);

  if (s.pointers.size === 2) {
    // 第二指落下 → 捏合：以当前 user/scale 为起点，单指手势无缝接管
    const [idA, idB] = [...s.pointers.keys()];
    const p1 = s.pointers.get(idA) ?? { x: ev.clientX, y: ev.clientY };
    const p2 = s.pointers.get(idB) ?? { x: ev.clientX, y: ev.clientY };
    ensureNormalized(s);
    s.gesture = {
      mode: 'pinch',
      idA,
      idB,
      startDist: Math.hypot(p2.x - p1.x, p2.y - p1.y) || 1,
      startMid: { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 },
      startScale: s.view.scale,
      startUser: s.view.user ?? matIdentity(),
    };
    return;
  }
  if (s.pointers.size > 2) return;

  ensureNormalized(s);
  if (s.view.scale > 1) {
    s.gesture = {
      mode: 'pan',
      pointerId: ev.pointerId,
      startX: ev.clientX,
      startY: ev.clientY,
      startUser: s.view.user ?? matIdentity(),
    };
  } else {
    s.gesture = { mode: 'dragclose', pointerId: ev.pointerId, startY: ev.clientY, dy: 0 };
  }
}

function onPointerMove(s: LightboxState, ev: PointerEvent): void {
  if (!s?.gesture || !s.pointers.has(ev.pointerId)) return;
  s.pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  const g = s.gesture;

  if (g.mode === 'pinch') {
    const p1 = s.pointers.get(g.idA);
    const p2 = s.pointers.get(g.idB);
    if (!p1 || !p2) return;
    const dist = Math.hypot(p2.x - p1.x, p2.y - p1.y) || 1;
    const mid = { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
    const next = clampScale(g.startScale * (dist / g.startDist));
    // 双指锚点：startMid 下的像素跟手移到当前 mid，同时按倍率缩放
    s.view.user = matMultiply(
      matTranslate(mid.x, mid.y),
      matMultiply(
        matScale(next / g.startScale),
        matMultiply(matTranslate(-g.startMid.x, -g.startMid.y), g.startUser),
      ),
    );
    s.view.scale = next;
    applyView(s, false);
    return;
  }
  if (g.pointerId !== ev.pointerId) return;

  if (g.mode === 'pan') {
    s.view.user = panBy(g.startUser, ev.clientX - g.startX, ev.clientY - g.startY);
    applyView(s, false);
    return;
  }

  // dragclose：图跟手指竖直移动，遮罩按行程淡出（120px 到位）
  const dy = ev.clientY - g.startY;
  g.dy = dy;
  const { m0 } = baseGeometry(s);
  s.img.style.transition = 'none';
  s.img.style.transform = matToCss(matMultiply(matTranslate(0, dy), m0));
  const progress = Math.min(Math.abs(dy) / DRAG_CLOSE_PX, 1);
  s.overlay.style.opacity = String(1 - progress);
}

function onPointerUp(s: LightboxState, actions: LightboxActions, ev: PointerEvent): void {
  s.pointers.delete(ev.pointerId);
  const g = s.gesture;
  if (!g) return;

  if (g.mode === 'pinch') {
    if (ev.pointerId !== g.idA && ev.pointerId !== g.idB) return;
    // 剩余一指 → 切换为 pan（重新取起点，避免跳变）；无剩余则结束
    const remaining = [...s.pointers.entries()][0];
    s.gesture = remaining
      ? {
          mode: 'pan',
          pointerId: remaining[0],
          startX: remaining[1].x,
          startY: remaining[1].y,
          startUser: s.view.user ?? matIdentity(),
        }
      : null;
    applyView(s, true); // 钳制回弹
    return;
  }
  if (g.pointerId !== ev.pointerId) return;
  s.gesture = null;

  if (g.mode === 'dragclose') {
    if (Math.abs(g.dy) >= DRAG_CLOSE_PX) {
      actions.close(false); // 行程到位 → 走飞回关闭
    } else {
      // 未到位 → 回弹：图归位、遮罩恢复不透明
      applyView(s, true);
      s.overlay.style.transition = 'opacity 180ms ease-out';
      s.overlay.style.opacity = '1';
    }
    return;
  }
  applyView(s, true); // pan 结束：边界钳制回弹
}
