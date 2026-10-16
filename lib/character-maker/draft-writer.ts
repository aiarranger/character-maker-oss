/** Debounced writes plus explicit lifecycle flushing. Construction and cleanup
 * never write: merely loading a draft must not overwrite a newer persisted one. */
export function createDraftWriter<T>(options: {
  getSnapshot: () => T | null;
  write: (snapshot: T) => void;
  setTimer: (callback: () => void, delay: number) => number;
  clearTimer: (timer: number) => void;
  onError: (error: unknown) => void;
  onSuccess?: () => void;
  delay?: number;
}) {
  let timer: number | null = null;
  let revision = 0, persistedRevision = 0;
  const cancel = () => {
    if (timer !== null) options.clearTimer(timer);
    timer = null;
  };
  const flush = () => {
    cancel();
    if (revision === persistedRevision) return;
    const writingRevision = revision;
    try {
      const snapshot = options.getSnapshot();
      if (snapshot === null) return; // Not hydrated, or name is not yet valid.
      options.write(snapshot);
      persistedRevision = writingRevision;
      options.onSuccess?.();
    } catch (error) {
      // Keep the edit pending so a later lifecycle event/edit can retry.
      options.onError(error);
    }
  };
  return {
    changed() {
      revision++;
      cancel();
      timer = options.setTimer(flush, options.delay ?? 400);
    },
    flush,
    // Call only after a synchronous write of the now-current draft succeeds.
    acceptPersisted() { persistedRevision = revision; cancel(); },
    // Effect cleanup cancels timing only. Never persist a stale unmount snapshot.
    cancel,
    hasPending() { return revision !== persistedRevision; },
  };
}
