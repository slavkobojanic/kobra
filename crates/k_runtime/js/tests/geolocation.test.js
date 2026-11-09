/**
 * @module k:/tests/geolocation.test.js
 *
 * `k.device.BrowserGeolocation` (docs/design/45-web-platform.md, "Geolocation";
 * docs/compiler/runtime.md §12.7) against a scripted `navigator.geolocation` and
 * `navigator.permissions` written from the Geolocation API and Permissions standards and each
 * engine's documented behaviour: no geolocation outside a secure context; the prompt shown by the
 * first position request; `GeolocationPositionError` codes 1 (`PERMISSION_DENIED`), 2
 * (`POSITION_UNAVAILABLE`) and 3 (`TIMEOUT`); a `PermissionStatus` whose `change` event follows
 * the site's settings (Chromium, Gecko) or doesn't fire (WebKit, re-read when the page becomes
 * visible); Gecko's "allow once", after which the Permissions API still says `prompt`; an engine
 * without the Permissions API; a `NaN` heading while the device is still.
 */
import { assert, assertEq, test, tick } from "k:testing";
import { BrowserGeolocation, position } from "k:/runtime/geolocation.js";
import { Instant, computed, flush, root, unitCase } from "k:runtime";

const PROMPT = unitCase(0);
const GRANTED = unitCase(1);
const DENIED = unitCase(2);
const COARSE = unitCase(0);
const PRECISE = unitCase(1);

/** A `GeolocationPosition` at `latitude`, `longitude`. */
function at(latitude, longitude, extra = {}) {
  return {
    coords: { latitude, longitude, accuracy: 25, altitude: null, heading: Number.NaN, speed: null, ...extra },
    timestamp: 1_700_000_000_000,
  };
}

/**
 * A scripted browser. `state` is the site's permission (`"prompt"`, `"granted"`, `"denied"`);
 * `answer` how the user answers the next prompt; `fix` the device's position (`null`: no fix,
 * error code 2); `fail` an error code the next request gets; `requests` records each
 * `getCurrentPosition`'s options and `watches` the open `watchPosition`s. `change(state)` changes
 * the setting as the site's settings would (firing `change` unless `firesChange` is false).
 */
function fakeBrowser({ permissions = true, firesChange = true, allowOnce = false } = {}) {
  const fake = {
    state: "prompt",
    answer: "granted",
    fix: at(52.52, 13.4),
    fail: null,
    prompts: 0,
    requests: [],
    watches: new Map(),
    cleared: [],
    queries: 0,
  };
  let nextId = 1;
  const status = { state: "prompt", onchange: null };
  const sync = () => {
    status.state = allowOnce && fake.state === "granted" ? "prompt" : fake.state;
  };
  fake.change = (state) => {
    fake.state = state;
    sync();
    if (firesChange) status.onchange?.();
    if (state === "denied") for (const w of fake.watches.values()) w.error({ code: 1 });
  };
  /** Whether a request may proceed, prompting as the browser would. */
  const allowed = () => {
    if (fake.state === "prompt") {
      fake.prompts++;
      fake.state = fake.answer;
      sync();
    }
    return fake.state === "granted";
  };
  fake.geolocation = {
    getCurrentPosition(ok, error, options) {
      fake.requests.push(options);
      queueMicrotask(() => {
        if (!allowed()) return error({ code: 1 });
        const code = fake.fail;
        fake.fail = null;
        if (code !== null) return error({ code });
        if (fake.fix === null) return error({ code: 2 });
        ok(fake.fix);
      });
    },
    watchPosition(ok, error, options) {
      const id = nextId++;
      fake.watches.set(id, { ok, error, options });
      if (allowed() && fake.fix !== null) queueMicrotask(() => fake.watches.get(id)?.ok(fake.fix));
      return id;
    },
    clearWatch(id) {
      fake.cleared.push(id);
      fake.watches.delete(id);
    },
  };
  fake.move = (p) => {
    fake.fix = p;
    for (const w of fake.watches.values()) w.ok(p);
  };
  fake.permissions = permissions
    ? {
        query: async (d) => {
          fake.queries++;
          if (d?.name !== "geolocation") throw new TypeError("unknown permission");
          sync();
          return status;
        },
      }
    : undefined;
  return fake;
}

/** Runs `f` in a browser with `fake`'s APIs, in a context that is secure or not. */
async function inBrowser(fake, secure, f) {
  const saved = [globalThis.navigator, globalThis.isSecureContext, globalThis.document];
  globalThis.navigator = { geolocation: fake?.geolocation, permissions: fake?.permissions };
  globalThis.isSecureContext = secure;
  const listeners = [];
  globalThis.document = {
    visibilityState: "visible",
    addEventListener: (type, f) => type === "visibilitychange" && listeners.push(f),
  };
  fake && (fake.becomeVisible = () => listeners.forEach((l) => l()));
  try {
    return await f();
  } finally {
    [globalThis.navigator, globalThis.isSecureContext, globalThis.document] = saved;
  }
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

/** `f()`'s thrown value, or `undefined`. */
function thrown(f) {
  try {
    f();
  } catch (e) {
    return e;
  }
  return undefined;
}

test("a browser position is a Position: unknown altitude, NaN heading and speed are nil", () => {
  const p = position(at(1.5, -2.5, { altitude: 34, speed: 0 }));
  assertEq(p.latitude, 1.5);
  assertEq(p.longitude, -2.5);
  assertEq(p.accuracyMeters, 25);
  assertEq(p.altitudeMeters, 34);
  assertEq(p.headingDegrees, null, "a NaN heading: not moving");
  assertEq(p.speedMetersPerSecond, 0);
  assert(p.measuredAt instanceof Instant);
  assertEq(p.measuredAt.ms, 1_700_000_000_000);
  assertEq(position(at(0, 0, { heading: 90 })).headingDegrees, 90);
});

test("outside a secure context there is no geolocation: unsupported, and nothing is asked", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, false, async () => {
    const g = BrowserGeolocation();
    assert(!g.isSupported);
    assertEq(g.permission, PROMPT);
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(0));
    assertEq(thrown(() => g.watch(COARSE)), unitCase(0));
    assertEq(await g.requestAccess(null), PROMPT);
    assertEq(fake.prompts, 0);
    assertEq(fake.queries, 0);
  });
  await inBrowser({ permissions: fake.permissions }, true, async () => {
    assert(!BrowserGeolocation().isSupported, "an engine without the API");
  });
});

test("the permission comes from the Permissions API, reactively, and follows its change event", async () => {
  const fake = fakeBrowser();
  fake.state = "granted";
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assert(g.isSupported);
    let seen = [];
    root(() => {
      const c = computed(() => g.permission);
      seen = [c];
    });
    assertEq(g.permission, PROMPT, ".prompt until the query answers");
    await tick();
    assertEq(seen[0].get(), GRANTED);
    fake.change("denied");
    flush();
    assertEq(seen[0].get(), DENIED, "the site's settings changed");
  });
});

test("reading before the user was asked is notAsked and never prompts", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(1));
    assertEq(fake.prompts, 0);
    assertEq(fake.requests.length, 0, "the browser was not called");
  });
});

test("requestAccess prompts with one position request, then reads use the grant", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assertEq(await g.requestAccess(null), GRANTED);
    assertEq(fake.prompts, 1);
    assertEq(g.permission, GRANTED);
    const p = await g.currentPosition(null, PRECISE, 60000, 5000);
    assertEq(p.latitude, 52.52);
    const options = fake.requests.at(-1);
    assertEq(options.enableHighAccuracy, true);
    assertEq(options.maximumAge, 60000);
    assertEq(options.timeout, 5000);
    assertEq((await g.currentPosition(null, COARSE, 0, 15000)).longitude, 13.4);
    assertEq(fake.requests.at(-1).enableHighAccuracy, false, ".coarse is not high accuracy");
    assertEq(await g.requestAccess(null), GRANTED, "already granted: no second prompt");
    assertEq(fake.prompts, 1);
  });
});

test("a refused prompt answers denied, and later reads are denied without asking", async () => {
  const fake = fakeBrowser();
  fake.answer = "denied";
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assertEq(await g.requestAccess(null), DENIED);
    assertEq(g.permission, DENIED);
    const before = fake.requests.length;
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(2));
    assertEq(fake.requests.length, before);
    assertEq(await g.requestAccess(null), DENIED, "only the site's settings can change it now");
    assertEq(fake.prompts, 1);
  });
});

test("no fix is unavailable, a slow fix is timeout, a revoked grant is denied", async () => {
  const fake = fakeBrowser();
  fake.state = "granted";
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    fake.fix = null;
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(3));
    fake.fix = at(1, 2);
    fake.fail = 3;
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(4));
    fake.fail = 1;
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(2));
    assertEq(g.permission, DENIED);
  });
});

test("a fix lost while asking still grants: the prompt was answered", async () => {
  const fake = fakeBrowser();
  fake.fix = null;
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assertEq(await g.requestAccess(null), GRANTED);
  });
});

test("Gecko's allow-once keeps the grant for the page though the Permissions API says prompt", async () => {
  const fake = fakeBrowser({ allowOnce: true });
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assertEq(await g.requestAccess(null), GRANTED);
    assertEq((await g.currentPosition(null, COARSE, 0, 15000)).latitude, 52.52);
    assertEq(fake.prompts, 1);
  });
});

test("without the Permissions API the answer to requestAccess is remembered", async () => {
  const fake = fakeBrowser({ permissions: false });
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    assertEq(g.permission, PROMPT, "the browser can't say");
    assertEq((await settle(g.currentPosition(null, COARSE, 0, 15000))).error, unitCase(1));
    assertEq(await g.requestAccess(null), GRANTED);
    assertEq((await g.currentPosition(null, COARSE, 0, 15000)).latitude, 52.52);
  });
});

test("WebKit fires no change event: the permission is read again when the page becomes visible", async () => {
  const fake = fakeBrowser({ firesChange: false });
  fake.state = "granted";
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    await tick();
    assertEq(g.permission, GRANTED);
    fake.change("denied");
    assertEq(g.permission, GRANTED, "no event");
    fake.becomeVisible();
    await tick();
    assertEq(g.permission, DENIED);
  });
});

test("watch throws synchronously unless granted", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    await tick();
    assertEq(thrown(() => g.watch(COARSE)), unitCase(1), ".prompt: not asked");
    fake.change("denied");
    await tick();
    assertEq(thrown(() => g.watch(COARSE)), unitCase(2));
    assertEq(fake.watches.size, 0, "no watch was started");
  });
});

test("a watch yields each position, skips a lost fix, and ends when the grant is revoked", async () => {
  const fake = fakeBrowser();
  fake.state = "granted";
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    await tick();
    const stream = g.watch(PRECISE);
    const [id, w] = [...fake.watches][0];
    assertEq(w.options.enableHighAccuracy, true);
    assertEq((await stream.next()).latitude, 52.52, "the first fix");
    w.error({ code: 2 });
    fake.move(at(48.85, 2.35));
    assertEq((await stream.next()).latitude, 48.85, "a lost fix yields nothing");
    fake.change("denied");
    flush();
    assertEq(await stream.next(), null, "ended");
    assertEq(g.permission, DENIED);
    assert(fake.cleared.includes(id), "the browser's watch is cleared");
  });
});

test("a watch ends and is cleared when its owner is disposed, or its loop is left", async () => {
  const fake = fakeBrowser();
  fake.state = "granted";
  await inBrowser(fake, true, async () => {
    const g = BrowserGeolocation();
    await tick();
    let stream = null;
    const dispose = root((d) => {
      stream = g.watch(COARSE);
      return d;
    });
    assertEq(fake.watches.size, 1);
    dispose();
    assertEq(fake.watches.size, 0, "cleared with its owner");
    await stream.next();
    assertEq(await stream.next(), null);
    const other = g.watch(COARSE);
    for await (const p of other) {
      assertEq(p.latitude, 52.52);
      break;
    }
    assertEq(fake.watches.size, 0, "leaving a for await closes it");
  });
});
