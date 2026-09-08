/** One owner for native route and theme snapshots, including interrupted callbacks. */
type TransitionKind = 'route' | 'theme';

export interface TransitionOwner {
  isCurrent(): boolean;
  attach(transition: ViewTransition): void;
  onCancel(cleanup: (nextKind: TransitionKind) => void): void;
  finish(): void;
}

interface ActiveOwner {
  cancel(nextKind: TransitionKind): void;
}

let activeOwner: ActiveOwner | undefined;

/**
 * Cancel the old owner synchronously, before the next caller prepares snapshots.
 * Cancelling an animation does not cancel its native update callback; callers must
 * guard late updates with isCurrent() and settle required work in onCancel().
 */
export function beginTransition(kind: TransitionKind): TransitionOwner {
  activeOwner?.cancel(kind);

  let state: 'active' | 'cancelled' | 'finished' = 'active';
  let transition: ViewTransition | undefined;
  let cancelledBy: TransitionKind = kind;
  const cleanups: Array<(nextKind: TransitionKind) => void> = [];
  const skip = (value: ViewTransition) => {
    try {
      value.skipTransition();
    } catch {
      // An already-disposed native transition must not block the next navigation.
    }
  };
  const owner: ActiveOwner = {
    cancel(nextKind) {
      if (state !== 'active') return;
      state = 'cancelled';
      cancelledBy = nextKind;
      if (activeOwner === owner) activeOwner = undefined;
      if (transition) skip(transition);
      for (const cleanup of cleanups.splice(0)) {
        try {
          cleanup(nextKind);
        } catch {
          // Cleanup failures must not prevent a newer owner from taking over.
        }
      }
    },
  };
  activeOwner = owner;

  return {
    isCurrent: () => state === 'active' && activeOwner === owner,
    attach(value) {
      // ready rejects on normal interruption; failed updates reject all three.
      // Observe each promise directly, without a rejecting finally() derivative.
      void value.ready.catch(() => {});
      void value.finished.catch(() => {});
      void value.updateCallbackDone?.catch(() => {});
      transition = value;
      if (state !== 'active' || activeOwner !== owner) skip(value);
    },
    onCancel(cleanup) {
      if (state === 'cancelled') cleanup(cancelledBy);
      else if (state === 'active') cleanups.push(cleanup);
    },
    finish() {
      if (state !== 'active') return;
      state = 'finished';
      cleanups.length = 0;
      if (activeOwner === owner) activeOwner = undefined;
    },
  };
}
