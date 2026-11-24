// All tabs retain a shared session lock. An updater temporarily releases its own
// lock and requires exclusive access, so another tab's drafts cannot be bypassed.
export function createUpdateGuard(manager: LockManager, key: string) {
  let ready: Promise<void> | undefined;
  let held: Promise<void> | undefined;
  let release: (() => void) | undefined;
  function hold(): Promise<void> {
    if (!ready)
      ready = new Promise<void>((resolve, reject) => {
        const waiting = manager.request(key, { mode: "shared" }, async () => {
          await new Promise<void>((done) => {
            release = done;
            resolve();
          });
        }).then(() => {});
        held = waiting;
        void waiting.catch(reject);
      });
    return ready;
  }
  async function leave() {
    release?.();
    await held;
    ready = undefined;
    held = undefined;
    release = undefined;
  }
  async function run<T>(task: () => Promise<T>): Promise<T> {
    await hold();
    await leave();
    try {
      return await manager.request(
        key,
        { mode: "exclusive", ifAvailable: true },
        async (lock) => {
          if (!lock)
            throw new Error(
              "Close other Tracksmith tabs before updating. Save or revert any edits there first.",
            );
          return task();
        },
      );
    } finally {
      await hold();
    }
  }
  return { hold, leave, run };
}
let guard: Promise<ReturnType<typeof createUpdateGuard>> | undefined;
function getGuard() {
  if (!guard)
    guard = (async () => {
      if (!navigator.locks)
        throw new Error(
          "This browser cannot coordinate update restarts. Use the downloaded installer instead.",
        );
      const token = sessionStorage.getItem("tracksmith.session") ?? "";
      const digest = await crypto.subtle.digest(
        "SHA-256",
        new TextEncoder().encode(token),
      );
      const scope = Array.from(new Uint8Array(digest), (b) =>
        b.toString(16).padStart(2, "0"),
      ).join("");
      return createUpdateGuard(navigator.locks, `tracksmith-tabs-${scope}`);
    })();
  return guard;
}
export async function holdUpdateTabLock() {
  if (navigator.locks) await (await getGuard()).hold();
}
export async function withUpdateTabLock<T>(task: () => Promise<T>) {
  return (await getGuard()).run(task);
}
