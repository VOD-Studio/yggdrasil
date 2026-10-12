import { BUTTON_ZOOM_STEP } from './geometry';
import type { LightboxActions, LightboxElements } from './types';

const ERROR_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M180-120q-24 0-42-18t-18-42v-600q0-24 18-42t42-18h600q24 0 42 18t18 42v600q0 24-18 42t-42 18H180Zm43-314 172-172 170 170 171-171 44 44v-217H180v303l43 43Zm-43 254h600v-298l-44-44-171 171-170-170-172 172-43-43v212Zm0 0v-298 60-362 600Z"/></svg>';

// ---- 工具栏图标（Material Symbols，标准 960 视口，fill=currentColor）----
// 源档案：public/icons/{zoom_in,zoom_out,rotate_90_degrees_cw,fit_screen,download,close}_24dp_E3E3E3_FILL0_wght400_GRAD0_opsz24.svg
const ZOOM_IN_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M796-121 533-384q-30 26-69.96 40.5Q423.08-329 378-329q-108.16 0-183.08-75Q120-479 120-585t75-181q75-75 181.5-75t181 75Q632-691 632-584.85 632-542 618-502q-14 40-42 75l264 262-44 44ZM377-389q81.25 0 138.13-57.5Q572-504 572-585t-56.87-138.5Q458.25-781 377-781q-82.08 0-139.54 57.5Q180-666 180-585t57.46 138.5Q294.92-389 377-389Zm-31-85v-82h-82v-60h82v-81h60v81h81v60h-81v82h-60Z"/></svg>';
const ZOOM_OUT_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M796-121 533-384q-30 26-70 40.5T378-329q-108 0-183-75t-75-181q0-106 75-181t182-75q106 0 180.5 75T632-585q0 43-14 83t-42 75l264 262-44 44ZM377-389q81 0 138-57.5T572-585q0-81-57-138.5T377-781q-82 0-139.5 57.5T180-585q0 81 57.5 138.5T377-389ZM275-556v-60h201v60H275Z"/></svg>';
const ROTATE_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M436-80q-73 0-137.5-28.5t-113-77q-48.5-48.5-77-113T80-436q0-146 105-251t251-105h42l-84-84 44-44 158 158-158 158-44-44 84-84h-42q-122 0-209 87t-87 209q0 122 87 209t209 87q54 0 101-14t89-47l42 44q-47 40-108 58.5T436-80Zm262-140L476-442l222-222 222 222-222 222Zm0-82 136-136-136-136-136 136 136 136Zm0-136Z"/></svg>';
const RESET_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M820-610v-130H690v-60h130q24 0 42 18t18 42v130h-60Zm-740 0v-130q0-24 18-42t42-18h130v60H140v130H80Zm610 450v-60h130v-130h60v130q0 24-18 42t-42 18H690Zm-550 0q-24 0-42-18t-18-42v-130h60v130h130v60H140Zm60-120v-400h560v400H200Zm60-60h440v-280H260v280Zm0 0v-280 280Z"/></svg>';
const DOWNLOAD_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="M480-313 287-506l43-43 120 120v-371h60v371l120-120 43 43-193 193ZM220-160q-24 0-42-18t-18-42v-143h60v143h520v-143h60v143q0 24-18 42t-42 18H220Z"/></svg>';
const CLOSE_ICON_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -960 960 960" fill="currentColor"><path d="m256-200-56-56 224-224-224-224 56-56 224 224 224-224 56 56-224 224 224 224-56 56-224-224-224 224Z"/></svg>';

export function createLightboxElements(
  altText: string,
  galleryLength: number,
  index: number | null,
  isSingle: boolean,
  actions: LightboxActions,
): LightboxElements {
  // 建 DOM
  const overlay = document.createElement('div');
  overlay.className = 'lightbox-overlay';
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');
  overlay.setAttribute('aria-label', '图片预览');
  overlay.setAttribute('tabindex', '-1');
  // 遮罩从 0 淡入：创建即隐藏，append 后立即启动淡入（见下方 rAF），
  // 与原图加载解耦。若加载期就保持 opacity:1，start() 为首帧硬切回 0 再
  // 淡入，用户会看到遮罩在打开瞬间闪一下；原图慢/失败时遮罩照常淡入可见，
  // 不留 opacity:0 的全屏层拦截页面交互。
  overlay.style.opacity = '0';

  const img = document.createElement('img');
  img.className = 'lightbox-img';
  img.setAttribute('alt', altText);
  // 加载前先占 0 尺寸，避免原图（可能数千 px）在加载期间撑大文档
  // 可滚动区、触发非预期的 scroll 事件。start() 拿到 natural 尺寸后再设真实值。
  img.style.width = '0px';
  img.style.height = '0px';

  const caption = document.createElement('figcaption');
  caption.className = 'lightbox-caption';
  caption.textContent = altText;
  if (!altText) caption.style.display = 'none';

  const counter = document.createElement('div');
  counter.className = 'lightbox-counter';
  if (isSingle || galleryLength === 0) {
    counter.style.display = 'none';
  } else {
    counter.textContent = `${(index ?? 0) + 1} / ${galleryLength}`;
  }

  // 错误态容器：原图加载失败或点击已知坏图（is-error）时显示原因，
  // 替代过去的立即 removeOverlay。pointer-events:none（CSS）让点击穿透到
  // overlay 照常触发关闭。
  const errorBox = document.createElement('div');
  errorBox.className = 'lightbox-error';
  errorBox.style.display = 'none';
  errorBox.innerHTML = `${ERROR_ICON_SVG}<p class="lightbox-error-text"></p><p class="lightbox-error-hint">按 Esc 或点击背景关闭</p>`;

  // 图集模式（>1 张）才加左右导航箭头；单张不显示。
  let prevBtn: HTMLButtonElement | null = null;
  let nextBtn: HTMLButtonElement | null = null;
  if (!isSingle && galleryLength > 1) {
    prevBtn = document.createElement('button');
    prevBtn.className = 'lightbox-nav lightbox-prev';
    prevBtn.setAttribute('type', 'button');
    prevBtn.setAttribute('aria-label', '上一张');
    prevBtn.textContent = '\u2039';

    nextBtn = document.createElement('button');
    nextBtn.className = 'lightbox-nav lightbox-next';
    nextBtn.setAttribute('type', 'button');
    nextBtn.setAttribute('aria-label', '下一张');
    nextBtn.textContent = '\u203a';
  }

  // ---- 查看器工具栏：毛玻璃胶囊（缩小/放大/旋转/重置/下载/关闭）----
  const toolbar = document.createElement('div');
  toolbar.className = 'lightbox-toolbar';
  toolbar.setAttribute('role', 'toolbar');
  toolbar.setAttribute('aria-label', '图片操作');

  // 按钮点击 stopPropagation：不冒泡到 overlay 触发关闭
  const makeTool = (aria: string, svg: string, onClick: () => void): HTMLButtonElement => {
    const btn = document.createElement('button');
    btn.className = 'lightbox-tool';
    btn.setAttribute('type', 'button');
    btn.setAttribute('aria-label', aria);
    btn.innerHTML = svg;
    btn.addEventListener('click', (ev: MouseEvent): void => {
      ev.stopPropagation();
      onClick();
    });
    return btn;
  };
  toolbar.appendChild(
    makeTool('缩小', ZOOM_OUT_ICON_SVG, () => {
      actions.zoomBy(1 / BUTTON_ZOOM_STEP, window.innerWidth / 2, window.innerHeight / 2, true);
    }),
  );
  toolbar.appendChild(
    makeTool('放大', ZOOM_IN_ICON_SVG, () => {
      actions.zoomBy(BUTTON_ZOOM_STEP, window.innerWidth / 2, window.innerHeight / 2, true);
    }),
  );
  toolbar.appendChild(makeTool('顺时针旋转 90 度', ROTATE_ICON_SVG, actions.rotateCW));
  toolbar.appendChild(makeTool('重置视图', RESET_ICON_SVG, () => actions.resetView(true)));

  const downloadLink = document.createElement('a');
  downloadLink.className = 'lightbox-tool';
  downloadLink.setAttribute('aria-label', '下载原图');
  downloadLink.innerHTML = DOWNLOAD_ICON_SVG;
  // 不 preventDefault：保留 download/新标签打开的默认行为；仅阻止冒泡关闭
  downloadLink.addEventListener('click', (ev: MouseEvent): void => {
    ev.stopPropagation();
  });
  toolbar.appendChild(downloadLink);

  toolbar.appendChild(
    makeTool('关闭', CLOSE_ICON_SVG, () => {
      actions.close(false);
    }),
  );

  // 缩放倍率徽标（操控时短暂显示）
  const badge = document.createElement('div');
  badge.className = 'lightbox-zoom-badge';
  badge.setAttribute('aria-hidden', 'true');

  overlay.appendChild(img);
  overlay.appendChild(caption);
  overlay.appendChild(counter);
  overlay.appendChild(errorBox);
  overlay.appendChild(toolbar);
  overlay.appendChild(badge);
  if (prevBtn) overlay.appendChild(prevBtn);
  if (nextBtn) overlay.appendChild(nextBtn);
  document.body.appendChild(overlay);

  return {
    overlay,
    img,
    caption,
    counter,
    errorBox,
    toolbar,
    badge,
    downloadLink,
    prevBtn,
    nextBtn,
  };
}
