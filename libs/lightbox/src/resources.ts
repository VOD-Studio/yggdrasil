/** Browser resources owned by one lightbox or image request. */
export class ResourceScope {
  disposed = false;
  private cleanups = new Set<() => void>();
  private timers = new Map<number, () => void>();

  private own(cleanup: () => void): () => void {
    const release = (): void => {
      this.cleanups.delete(release);
      cleanup();
    };
    if (this.disposed) release();
    else this.cleanups.add(release);
    return release;
  }

  listen<E extends Event>(
    target: EventTarget,
    event: string,
    listener: (event: E) => void,
    options?: AddEventListenerOptions,
  ): () => void {
    if (this.disposed) return () => {};
    target.addEventListener(event, listener as EventListener, options);
    return this.own(() => target.removeEventListener(event, listener as EventListener, options));
  }

  timeout(callback: () => void, delay: number): number {
    if (this.disposed) return 0;
    const id = window.setTimeout(() => {
      this.clearTimeout(id);
      callback();
    }, delay);
    const release = this.own(() => {
      window.clearTimeout(id);
      this.timers.delete(id);
    });
    this.timers.set(id, release);
    return id;
  }

  clearTimeout(id: number | undefined): void {
    if (id !== undefined) this.timers.get(id)?.();
  }

  frame(callback: () => void): void {
    if (this.disposed) return;
    const id = requestAnimationFrame(() => {
      release();
      callback();
    });
    const release = this.own(() => cancelAnimationFrame(id));
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    for (const release of [...this.cleanups]) release();
  }
}
