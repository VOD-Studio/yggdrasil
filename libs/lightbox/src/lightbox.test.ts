/**
 * Lightbox 行为回归测试（happy-dom 真实 DOM，黑盒驱动）。
 *
 * 通过唯一公开入口 window.__initLightbox 驱动，构造 .blur-img DOM，
 * 模拟点击/键盘/滚动，断言 overlay 出现/消失、counter 文本、originNode 捕获。
 *
 * 目的：钉住高风险路径（循环闭包捕获 idx、图集 gotoIndex 循环边界、
 * 关闭清理），供后续现代化重构（var→const、拆 IIFE、for→for..of）做回归防线。
 *
 * 不覆盖：飞行动画的几何计算（依赖 img naturalWidth + load 事件，happy-dom
 * 不真实加载图片），那部分由 geometry.test.ts 的纯函数测试覆盖。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import './index';

// ============ 测试夹具 ============

/**
 * 构造一个 .blur-img 容器（图集成员）。
 * full img 带 data-src + alt，模拟文章正文图结构。
 */
function makeGalleryImage(dataSrc: string, alt: string): HTMLElement {
  const container = document.createElement('div');
  container.className = 'blur-img';
  container.innerHTML = `
    <img class="blur-img-placeholder" src="${dataSrc}?w=20" alt="${alt}">
    <img class="blur-img-full" data-src="${dataSrc}" alt="${alt}">
  `;
  return container;
}

/** 构造单张图（封面，带 lightbox-single）。 */
function makeSingleImage(dataSrc: string, alt: string): HTMLElement {
  const container = makeGalleryImage(dataSrc, alt);
  container.classList.add('lightbox-single');
  return container;
}
/** 构造原生 <img> 外链图（无 .blur-img 包裹）。 */
function makeExternalImage(src: string, alt: string): HTMLImageElement {
  const img = document.createElement('img');
  img.src = src;
  img.setAttribute('alt', alt);
  return img;
}

/**
 * 把若干图片挂到一个 root 容器下，再挂到 document.body。
 * 返回 root 以便选择器命中。
 */
function mountRoot(images: HTMLElement[]): HTMLElement {
  const root = document.createElement('div');
  root.className = 'post-content';
  for (const img of images) root.appendChild(img);
  document.body.appendChild(root);
  return root;
}

/** 取当前 overlay（灯箱打开时存在）。 */
function getOverlay(): HTMLElement | null {
  return document.querySelector('.lightbox-overlay');
}

/** 取灯箱图（.lightbox-img）。 */
function getLightboxImg(): HTMLImageElement | null {
  return document.querySelector('.lightbox-img');
}

/** 取计数器。 */
function getCounter(): HTMLElement | null {
  return document.querySelector('.lightbox-counter');
}

/** stub 灯箱图的 natural 尺寸与 complete（happy-dom 不加载真实图片）。 */
function stubNatural(img: HTMLImageElement, w: number, h: number): void {
  Object.defineProperty(img, 'naturalWidth', { configurable: true, value: w });
  Object.defineProperty(img, 'naturalHeight', { configurable: true, value: h });
  Object.defineProperty(img, 'complete', { configurable: true, value: true });
}

/** 模拟元素 click（真实事件派发，触发 addEventListener('click')）。 */
function clickEl(el: Element): void {
  el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

/** 模拟 keydown。 */
function pressKey(key: string): void {
  document.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
}

// ============ 测试 ============

describe('lightbox 黑盒行为', () => {
  beforeEach(() => {
    // 每个测试干净的 DOM + matchMedia（prefersReducedMotion 读它）
    document.body.innerHTML = '';
    // happy-dom 的 matchMedia 返回值默认 matches=false，reduced-motion 关闭，
    // 这样打开走 double-rAF 动画路径（更接近真实）。但我们用 fake timers 跳过动画。
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
  });

  describe('循环闭包捕获 idx（gallery 绑定）', () => {
    it('点击第 1/2/3 张图，counter 分别显示 1/3、2/3、3/3', () => {
      const imgs = [
        makeGalleryImage('/a.webp', '图A'),
        makeGalleryImage('/b.webp', '图B'),
        makeGalleryImage('/c.webp', '图C'),
      ];
      mountRoot(imgs);
      window.__initLightbox('.post-content');

      // 逐张点击，验证捕获的 idx 正确（这是 var g + IIFE 的核心风险点）
      clickEl(imgs[0]);
      expect(getCounter()?.textContent).toBe('1 / 3');
      pressKey('Escape'); // 关闭
      vi.advanceTimersByTime(300); // 等 close 的 280ms 兜底

      clickEl(imgs[1]);
      expect(getCounter()?.textContent).toBe('2 / 3');
      pressKey('Escape');
      vi.advanceTimersByTime(300);

      clickEl(imgs[2]);
      expect(getCounter()?.textContent).toBe('3 / 3');
    });

    it('点图片时 preventDefault（阻止默认导航/拖拽）', () => {
      const img = makeGalleryImage('/a.webp', '图A');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      const ev = new MouseEvent('click', { bubbles: true, cancelable: true });
      img.dispatchEvent(ev);
      expect(ev.defaultPrevented).toBe(true);
    });
  });

  describe('点击打开灯箱（overlay 创建 + originNode 捕获）', () => {
    it('点击 gallery 图后出现 overlay，img src 为原图（去 query）', () => {
      const img = makeGalleryImage('/uploads/x.webp?w=800', '描述');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      expect(getOverlay()).toBeNull(); // 打开前无 overlay
      clickEl(img);
      expect(getOverlay()).not.toBeNull(); // 打开后有 overlay
      // origSrc = data-src 去 query
      expect(getLightboxImg()?.getAttribute('src')).toBe('/uploads/x.webp');
    });

    it('caption 显示 alt 文本', () => {
      const img = makeGalleryImage('/a.webp', '我的描述文字');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      const caption = document.querySelector('.lightbox-caption');
      expect(caption?.textContent).toBe('我的描述文字');
    });
    it('素材页点击后原图尚未加载时也应立即显示灯箱', () => {
      const img = makeGalleryImage('/uploads/asset.webp?thumb=300x300', '素材');
      const root = mountRoot([img]);
      root.classList.add('assets-lightbox');
      window.__initLightbox('.assets-lightbox');

      clickEl(img);

      // 遮罩 DOM 立即出现（不等原图），opacity 从 0 起淡入，动画结束到 1
      const overlay = getOverlay();
      expect(overlay).not.toBeNull();
      vi.advanceTimersByTime(64);
      expect(overlay?.style.opacity).toBe('1');
    });
    it('素材原图加载失败时显示错误态而非闪退，Esc 仍可关闭', () => {
      const img = makeGalleryImage('/uploads/missing.webp?thumb=300x300', '损坏素材');
      const root = mountRoot([img]);
      root.classList.add('assets-lightbox');
      window.__initLightbox('.assets-lightbox');

      clickEl(img);
      const lightboxImg = getLightboxImg();
      expect(lightboxImg).not.toBeNull();
      lightboxImg?.dispatchEvent(new Event('error'));

      // overlay 保留并展示错误态（含默认文案），不再「闪一下就消失」
      const overlay = getOverlay();
      expect(overlay).not.toBeNull();
      const errBox = overlay?.querySelector('.lightbox-error') as HTMLElement | null;
      expect(errBox).not.toBeNull();
      expect(errBox?.style.display).toBe('');
      expect(errBox?.textContent).toContain('图片加载失败');

      pressKey('Escape');
      vi.advanceTimersByTime(300);
      expect(getOverlay()).toBeNull();
    });

    it('单张图（lightbox-single）打开时 counter 隐藏', () => {
      const img = makeSingleImage('/cover.webp', '封面');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      expect(getCounter()?.style.display).toBe('none');
    });
  });

  describe('打开动画遮罩不闪烁（opacity 单调）', () => {
    /**
     * 记录点击 → 原图 load → rAF 推进全过程中 overlay 的 inline opacity 序列。
     * 遮罩一旦对用户可见就不得再变透明：创建时 opacity=1（加载期实黑）→
     * start() 硬切 0（无 transition）→ rAF 再淡入，就是用户看到的「打开时闪一下」。
     */
    const expectMonotonic = (seq: number[]): void => {
      for (let i = 1; i < seq.length; i++) {
        expect(seq[i]).toBeGreaterThanOrEqual(seq[i - 1]);
      }
    };

    it('慢原图路径：遮罩从点击到动画结束 opacity 不回退，且最终为 1', () => {
      const img = makeGalleryImage('/uploads/x.webp?w=800', '图');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      const overlay = getOverlay()!;
      const seq: number[] = [Number(overlay.style.opacity || '0')];

      // 原图未缓存（complete=false）：走 load 监听路径，模拟真实慢加载
      const lbImg = getLightboxImg()!;
      stubNatural(lbImg, 1200, 800);
      lbImg.dispatchEvent(new Event('load'));
      seq.push(Number(overlay.style.opacity || '0'));

      vi.advanceTimersByTime(64); // 推进 double-rAF（淡入动画启动）
      seq.push(Number(overlay.style.opacity || '0'));

      expectMonotonic(seq);
      expect(seq[seq.length - 1]).toBe(1); // 动画结束遮罩必须可见
    });

    it('缓存命中路径：遮罩 opacity 同样不回退', () => {
      const img = makeGalleryImage('/uploads/x.webp?w=800', '图');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      const overlay = getOverlay()!;
      const seq: number[] = [Number(overlay.style.opacity || '0')];

      vi.advanceTimersByTime(64);
      seq.push(Number(overlay.style.opacity || '0'));

      expectMonotonic(seq);
      expect(seq[seq.length - 1]).toBe(1);
    });
  });

  describe('关闭灯箱', () => {
    it('Esc 关闭后 overlay 从 DOM 移除', () => {
      const img = makeGalleryImage('/a.webp', 'A');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      expect(getOverlay()).not.toBeNull();
      pressKey('Escape');
      // closeLightbox 走 transitionend（happy-dom 不触发）+ 280ms 兜底
      vi.advanceTimersByTime(300);
      expect(getOverlay()).toBeNull();
    });

    it('点 overlay 背景关闭（点图片本身不关）', () => {
      const img = makeGalleryImage('/a.webp', 'A');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      const overlay = getOverlay()!;
      const lbImg = getLightboxImg()!;

      // 点图片本身 → 不关闭（箭头在图上，避免误关）
      clickEl(lbImg);
      expect(getOverlay()).not.toBeNull();

      // 点背景 → 关闭
      clickEl(overlay);
      vi.advanceTimersByTime(300);
      expect(getOverlay()).toBeNull();
    });
  });

  describe('图集 gotoIndex 循环边界', () => {
    it('第 1 张按 ← 跳到最后一张（循环）', () => {
      const imgs = [makeGalleryImage('/a.webp', 'A'), makeGalleryImage('/b.webp', 'B')];
      mountRoot(imgs);
      window.__initLightbox('.post-content');

      clickEl(imgs[0]);
      expect(getCounter()?.textContent).toBe('1 / 2');

      pressKey('ArrowLeft'); // 从第 1 张往左 → 循环到最后
      vi.advanceTimersByTime(200); // gotoIndex 的 150ms 淡出 + 淡入
      expect(getCounter()?.textContent).toBe('2 / 2');
    });

    it('最后一张按 → 跳到第 1 张（循环）', () => {
      const imgs = [makeGalleryImage('/a.webp', 'A'), makeGalleryImage('/b.webp', 'B')];
      mountRoot(imgs);
      window.__initLightbox('.post-content');

      clickEl(imgs[1]); // 最后一张
      expect(getCounter()?.textContent).toBe('2 / 2');

      pressKey('ArrowRight'); // 循环到第 1 张
      vi.advanceTimersByTime(200);
      expect(getCounter()?.textContent).toBe('1 / 2');
    });

    it('切换后 originNode 更新：关闭后焦点归还到新图', () => {
      const imgs = [makeGalleryImage('/a.webp', 'A'), makeGalleryImage('/b.webp', 'B')];
      mountRoot(imgs);
      window.__initLightbox('.post-content');

      clickEl(imgs[0]);
      pressKey('ArrowRight'); // 切到 imgs[1]
      vi.advanceTimersByTime(200);

      // originNode 应已更新为 imgs[1]，关闭后焦点归还给 imgs[1] 的 full img
      pressKey('Escape');
      vi.advanceTimersByTime(300);

      const fullB = imgs[1].querySelector('.blur-img-full');
      expect(document.activeElement).toBe(fullB);
    });

    it('切换后按新图宽高比重算几何（不沿用第一张的 target/scale）', () => {
      const imgs = [makeGalleryImage('/a.webp', 'A'), makeGalleryImage('/b.webp', 'B')];
      mountRoot(imgs);
      window.__initLightbox('.post-content');

      clickEl(imgs[0]);
      const lbImg = getLightboxImg()!;
      // happy-dom 不真实加载图片：手动 stub 第 1 张 natural 尺寸并派发 load，
      // 让 openLightbox 的 start() 走完（state.target/baseW 按第 1 张建立）。
      stubNatural(lbImg, 1000, 500);
      lbImg.dispatchEvent(new Event('load'));
      vi.advanceTimersByTime(50); // double-rAF 在 fake timers 下同步推进

      // 切到第 2 张前，先把 natural stub 成新图尺寸（竖图），模拟缓存命中
      pressKey('ArrowRight');
      stubNatural(lbImg, 500, 1000);
      vi.advanceTimersByTime(200); // 150ms 淡出后 swap 同步执行

      // 几何必须按 500x1000 重算：布局盒 = fitCentered 目标尺寸（宽高比 0.5），
      // transform 归位 scale(1,1)。修复前这里仍是第 1 张的 width/height/scale。
      const w = parseFloat(lbImg.style.width);
      const h = parseFloat(lbImg.style.height);
      expect(w).toBeGreaterThan(0);
      expect(w / h).toBeCloseTo(0.5, 2);
      expect(lbImg.style.transform).toContain('scale(1,1)');
    });
  });

  describe('重复初始化幂等（SPA 数据刷新场景）', () => {
    it('同一批节点重复 __initLightbox，点击只创建一次 overlay', () => {
      const img = makeGalleryImage('/a.webp', 'A');
      mountRoot([img]);
      window.__initLightbox('.post-content');
      // 模拟 /admin/assets 刷新后 Dioxus 复用 DOM 节点导致的重复初始化
      window.__initLightbox('.post-content');

      const appendSpy = vi.spyOn(document.body, 'appendChild');
      clickEl(img);
      // 无守卫时两个 click 监听先后触发 openLightbox → overlay 被 append 两次
      const overlayAppends = appendSpy.mock.calls.filter(
        (c) => c[0] instanceof HTMLElement && c[0].classList.contains('lightbox-overlay'),
      );
      expect(overlayAppends).toHaveLength(1);
      appendSpy.mockRestore();
    });
  });

  describe('加载失败错误态（is-error 标记与灯箱错误提示）', () => {
    /** 取容器内的展示层 img 并驱动一次失败。 */
    const fullImgOf = (container: HTMLElement): HTMLImageElement =>
      container.querySelector('.blur-img-full') as HTMLImageElement;

    it('缩略图失败：退避重试耗尽后才标 is-error 并补默认文案', () => {
      const container = makeGalleryImage('/uploads/gone.webp?thumb=300x300', '丢失');
      mountRoot([container]);
      window.__initLightbox('.post-content');

      const full = fullImgOf(container);
      full.src = full.getAttribute('data-src')!;

      // 前三次失败各自排程一次退避重试，不标错
      for (const delay of [1000, 2000, 4000]) {
        full.dispatchEvent(new Event('error'));
        expect(container.classList.contains('is-error')).toBe(false);
        vi.advanceTimersByTime(delay); // 触发重试重设 src
      }
      // 重试耗尽后的第四次失败 → 永久错误态
      full.dispatchEvent(new Event('error'));
      expect(container.classList.contains('is-error')).toBe(true);
      expect(container.getAttribute('data-error-text')).toBe('图片加载失败');
    });

    it('重试期间加载成功（429 自愈）：正常 is-loaded，不标 is-error', () => {
      const container = makeGalleryImage('/uploads/slow.webp?thumb=300x300', '慢图');
      mountRoot([container]);
      window.__initLightbox('.post-content');

      const full = fullImgOf(container);
      full.src = full.getAttribute('data-src')!;
      full.dispatchEvent(new Event('error'));
      vi.advanceTimersByTime(1000); // 第一次退避到期，重设 src

      full.dispatchEvent(new Event('load')); // 重试成功
      expect(container.classList.contains('is-loaded')).toBe(true);
      expect(container.classList.contains('is-error')).toBe(false);
    });

    it('点击已标 is-error 的图：直接错误态（用容器定制文案），不请求原图', () => {
      const container = makeGalleryImage('/uploads/lost.webp?thumb=300x300', '丢失素材');
      container.classList.add('is-error');
      container.setAttribute('data-error-text', '本地文件已丢失');
      mountRoot([container]);
      window.__initLightbox('.post-content');

      clickEl(container);
      const overlay = getOverlay();
      expect(overlay).not.toBeNull();
      const errBox = overlay?.querySelector('.lightbox-error') as HTMLElement | null;
      expect(errBox?.style.display).toBe('');
      expect(errBox?.textContent).toContain('本地文件已丢失');
      // 不发起原图请求
      expect(getLightboxImg()?.getAttribute('src')).toBeNull();
    });

    it('图集切换到坏图显示错误态，再切回好图恢复正常', () => {
      const good = makeGalleryImage('/uploads/good.webp', '好图');
      const bad = makeGalleryImage('/uploads/bad.webp', '坏图');
      bad.classList.add('is-error');
      bad.setAttribute('data-error-text', '本地文件已丢失');
      mountRoot([good, bad]);
      window.__initLightbox('.post-content');

      clickEl(good);
      const lbImg = getLightboxImg()!;
      stubNatural(lbImg, 800, 600);
      lbImg.dispatchEvent(new Event('load'));
      vi.advanceTimersByTime(50);

      // → 坏图：错误态 + counter 照常更新
      pressKey('ArrowRight');
      vi.advanceTimersByTime(200);
      const errBox = document.querySelector('.lightbox-error') as HTMLElement;
      expect(errBox.style.display).toBe('');
      expect(errBox.textContent).toContain('本地文件已丢失');
      expect(getCounter()?.textContent).toBe('2 / 2');

      // → 循环回好图：错误态隐藏，src 恢复请求
      pressKey('ArrowRight');
      vi.advanceTimersByTime(200);
      expect(errBox.style.display).toBe('none');
      expect(getLightboxImg()?.getAttribute('src')).toContain('/uploads/good.webp');
      expect(getCounter()?.textContent).toBe('1 / 2');
    });
  });

  describe('单张图不参与图集切换', () => {
    it('单张图打开后按 ←/→ 不切换（无 counter、无箭头）', () => {
      const img = makeSingleImage('/cover.webp', '封面');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      // 单张模式无导航箭头
      expect(document.querySelector('.lightbox-prev')).toBeNull();
      expect(document.querySelector('.lightbox-next')).toBeNull();

      // 按 ← 不报错也不改变状态（gotoIndex 早返）
      pressKey('ArrowLeft');
      vi.advanceTimersByTime(200);
      expect(getOverlay()).not.toBeNull(); // 仍打开
    });
  });
  describe('外链图片 (HTMLImageElement) 点击放大与混合图集', () => {
    it('单独外链图片点击触发灯箱打开，完整保留 URL，关闭后焦点归还', () => {
      const extUrl = 'https://example.com/photo.jpg?token=xyz';
      const img = makeExternalImage(extUrl, '外链图');
      mountRoot([img]);
      window.__initLightbox('.post-content');

      clickEl(img);
      const overlay = getOverlay();
      expect(overlay).not.toBeNull();

      const lbImg = getLightboxImg();
      expect(lbImg).not.toBeNull();
      expect(lbImg?.src).toContain(extUrl);

      // 关闭灯箱
      pressKey('Escape');
      vi.advanceTimersByTime(300);

      expect(getOverlay()).toBeNull();
      expect(document.activeElement).toBe(img);
    });

    it('混合图集（本地 .blur-img + 外链 <img>）按 DOM 顺序统一编号并完美切换', () => {
      const img1 = makeGalleryImage('/uploads/a.webp', '图A');
      const img2 = makeExternalImage('https://example.com/b.png?w=800', '图B');
      mountRoot([img1, img2]);
      window.__initLightbox('.post-content');

      // 点击第 1 张（本地图）
      clickEl(img1);
      expect(getCounter()?.textContent).toBe('1 / 2');

      // 按 → 切换到第 2 张（外链图）
      pressKey('ArrowRight');
      const lbImg = getLightboxImg()!;
      stubNatural(lbImg, 800, 600);
      vi.advanceTimersByTime(200);

      expect(getCounter()?.textContent).toBe('2 / 2');
      expect(lbImg.src).toContain('https://example.com/b.png?w=800');

      // 按 → 循环切回第 1 张
      pressKey('ArrowRight');
      vi.advanceTimersByTime(200);
      expect(getCounter()?.textContent).toBe('1 / 2');
      expect(lbImg.src).toContain('/uploads/a.webp');
    });
  });
});
