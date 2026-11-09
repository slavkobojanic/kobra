/**
 * @module k:/tests/media.test.js
 *
 * `k.media` in the browser (docs/design/46-media-and-rtc.md; docs/compiler/runtime.md §12.14)
 * against a scripted `navigator.mediaDevices`, `navigator.permissions`, `MediaStream`,
 * `MediaRecorder` and `Blob`, written from the Media Capture and Streams, MediaStream Recording
 * and Permissions standards and Chromium's behaviour: no `mediaDevices` outside a secure context;
 * `getUserMedia` rejecting with `NotAllowedError`, `NotFoundError`, `NotReadableError` and
 * `OverconstrainedError` (whose `constraint` names the setting); a track's `ended` event when
 * its device goes; `MediaRecorder.isTypeSupported` and `dataavailable` chunks; and the `video`
 * and `audio` elements' binding (`playback.js`).
 */
import { assert, assertDeepEq, assertEq, installDom, test, tick } from "k:testing";
import {
  BrowserMediaDevices,
  BrowserRecorder,
  MediaController,
  captureError,
  mediaStream,
  mediaTrack,
  recordingType,
} from "k:/runtime/media.js";
import { mediaElement } from "k:/runtime/playback.js";
import { CANCELLED, flush, root, unitCase } from "k:runtime";

const PROMPT = unitCase(0);
const GRANTED = unitCase(1);
const DENIED = unitCase(2);
/** `CaptureRequest.microphone`, `.camera`, and a chosen microphone. */
const MICROPHONE = { $: 0, _0: { device: null, echoCancellation: true, noiseSuppression: true, autoGainControl: true } };
const CAMERA = { $: 1, _0: { device: null, facing: null, size: null, frameRate: null } };
const chosen = (id) => ({ $: 0, _0: { ...MICROPHONE._0, device: { value: id } } });

/** A scripted `MediaStreamTrack`. */
function fakeTrack(kind, deviceId = `${kind}-1`) {
  const t = {
    kind,
    label: `Fake ${kind}`,
    enabled: true,
    muted: false,
    stopped: false,
    stop() {
      t.stopped = true;
    },
    getSettings: () => ({ deviceId, sampleRate: kind === "audio" ? 48000 : undefined }),
  };
  return t;
}

/**
 * A scripted browser. `state` holds each permission (`camera`, `microphone`); `answer` how the
 * user answers a prompt; `fail` an error the next `getUserMedia` rejects with; `calls` counts
 * `getUserMedia`; `devices` the `MediaDeviceInfo`s.
 */
function fakeBrowser() {
  const fake = {
    state: { camera: "prompt", microphone: "prompt" },
    answer: "granted",
    fail: null,
    calls: 0,
    hold: null,
    devices: [
      { kind: "audioinput", deviceId: "mic-1", label: "" },
      { kind: "videoinput", deviceId: "cam-1", label: "" },
      { kind: "audiooutput", deviceId: "out-1", label: "" },
    ],
  };
  // A `PermissionStatus` is live: its state is the site's setting now.
  const status = {
    camera: { get state() { return fake.state.camera; }, onchange: null },
    microphone: { get state() { return fake.state.microphone; }, onchange: null },
  };
  fake.mediaDevices = {
    async getUserMedia(c) {
      fake.calls++;
      if (fake.hold) await fake.hold;
      const names = [c.audio && "microphone", c.video && "camera"].filter(Boolean);
      for (const n of names) if (fake.state[n] === "prompt") fake.state[n] = fake.answer;
      if (names.some((n) => fake.state[n] !== "granted")) throw { name: "NotAllowedError" };
      const e = fake.fail;
      fake.fail = null;
      if (e) throw e;
      const exact = c.audio?.deviceId?.exact;
      if (exact && !fake.devices.some((d) => d.deviceId === exact)) throw { name: "OverconstrainedError", constraint: "deviceId" };
      const tracks = [c.audio && fakeTrack("audio", exact ?? "mic-1"), c.video && fakeTrack("video", "cam-1")].filter(Boolean);
      return { getTracks: () => tracks };
    },
    async enumerateDevices() {
      return fake.devices;
    },
    addEventListener() {},
  };
  fake.permissions = {
    query: async (d) => status[d.name],
  };
  return fake;
}

/** Runs `f` in a browser with `fake`'s APIs, in a context that is secure or not. */
async function inBrowser(fake, secure, f) {
  const keys = ["navigator", "isSecureContext", "setTimeout", "clearTimeout"];
  const saved = keys.map((k) => globalThis[k]);
  globalThis.navigator = { mediaDevices: fake?.mediaDevices, permissions: fake?.permissions };
  globalThis.isSecureContext = secure;
  const timers = [];
  globalThis.setTimeout = (f) => timers.push(f);
  globalThis.clearTimeout = () => {};
  try {
    return await f(timers);
  } finally {
    keys.forEach((k, i) => (globalThis[k] = saved[i]));
  }
}

/** `f()`'s thrown value, or `undefined`. */
function thrown(f) {
  try {
    f();
  } catch (e) {
    return e;
  }
  return undefined;
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

test("outside a secure context nothing can be captured, and nothing is asked", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, false, async () => {
    const m = BrowserMediaDevices();
    assert(!m.isSupported);
    assertEq((await settle(m.capture(null, MICROPHONE))).error, unitCase(0));
    assertEq(await m.requestAccess(null, MICROPHONE), PROMPT);
    assertEq(fake.calls, 0);
  });
});

test("a capture before the user was asked is notAsked, and the browser isn't called", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const m = BrowserMediaDevices();
    assertEq((await settle(m.capture(null, MICROPHONE))).error, unitCase(1));
    fake.state.camera = "denied";
    assertEq((await settle(m.capture(null, CAMERA))).error, unitCase(2));
    assertEq(fake.calls, 0);
  });
});

test("requestAccess opens the devices once; a capture right after takes the stream over", async () => {
  const fake = fakeBrowser();
  await inBrowser(fake, true, async (timers) => {
    const m = BrowserMediaDevices();
    assertEq(await m.requestAccess(null, MICROPHONE), GRANTED);
    assertEq(m.microphonePermission, GRANTED);
    assertEq(m.cameraPermission, PROMPT, "only what was asked for");
    const stream = (await settle(m.capture(null, MICROPHONE))).ok;
    assertEq(fake.calls, 1, "no second getUserMedia within 5 seconds");
    assertEq(stream.audioTracks.length, 1);
    assert(stream.isLive && stream.isLocal);
    assertDeepEq(stream.audioTracks[0].device, { value: "mic-1" });
    timers.forEach((t) => t());
    assert(!stream.$tracks[0].$t.stopped, "the hold's timer leaves a taken-over stream alone");
    assertEq(await m.requestAccess(null, MICROPHONE), GRANTED, "already granted: no prompt");
    assertEq(fake.calls, 1);
  });
});

test("a refusal is denied, and requestAccess answers it without asking again", async () => {
  const fake = fakeBrowser();
  fake.answer = "denied";
  await inBrowser(fake, true, async () => {
    const m = BrowserMediaDevices();
    assertEq(await m.requestAccess(null, MICROPHONE), DENIED);
    assertEq(await m.requestAccess(null, MICROPHONE), DENIED);
    assertEq(fake.calls, 1);
  });
});

test("browser errors map to CaptureError cases", () => {
  assertEq(captureError({ name: "NotAllowedError" }, false), unitCase(2));
  assertEq(captureError({ name: "NotAllowedError" }, true), unitCase(3), "the screen picker closed");
  assertEq(captureError({ name: "NotFoundError" }, false), unitCase(4));
  assertEq(captureError({ name: "NotReadableError" }, false), unitCase(5));
  assertEq(captureError({ name: "AbortError" }, false), unitCase(5));
  assertEq(captureError({ name: "OverconstrainedError", constraint: "deviceId" }, false), unitCase(4));
  assertDeepEq(captureError({ name: "OverconstrainedError", constraint: "width" }, false), { $: 6, setting: "width" });
  assertEq(captureError(new TypeError("x"), false), unitCase(0));
});

test("a chosen device that is gone is notFound, never another device", async () => {
  const fake = fakeBrowser();
  fake.state.microphone = "granted";
  await inBrowser(fake, true, async () => {
    const m = BrowserMediaDevices();
    assertEq((await settle(m.capture(null, chosen("gone")))).error, unitCase(4));
  });
});

test("the owner's cancellation stops the stream; one arriving after it is stopped at once", async () => {
  const fake = fakeBrowser();
  fake.state.microphone = "granted";
  await inBrowser(fake, true, async () => {
    const m = BrowserMediaDevices();
    const owner = new AbortController();
    const stream = (await settle(m.capture(owner.signal, MICROPHONE))).ok;
    owner.abort();
    assert(!stream.isLive);
    assertDeepEq(stream.audioTracks[0].state, { $: 1, _0: unitCase(0) }, "ended: stopped");
    assert(stream.$tracks[0].$t.stopped, "the device is released");
    let release;
    fake.hold = new Promise((r) => (release = r));
    const late = new AbortController();
    const pending = settle(m.capture(late.signal, MICROPHONE));
    await tick();
    late.abort();
    release();
    assertEq((await pending).error, CANCELLED, "no K code sees it");
  });
});

test("a track ends with deviceLost when its device goes, and devices are listed without labels", async () => {
  const t = fakeTrack("audio");
  const track = mediaTrack(t);
  const ended = [];
  track.$ends.add((why) => ended.push(why));
  t.onended();
  assertDeepEq(track.state, { $: 1, _0: unitCase(1) });
  assertDeepEq(ended, [1]);
  const screen = mediaTrack(fakeTrack("video"), { screen: true });
  screen.$t.onended();
  assertDeepEq(screen.state, { $: 1, _0: unitCase(2) }, "Stop sharing: endedByUser");
  assertEq(screen.device, null);
  const fake = fakeBrowser();
  await inBrowser(fake, true, async () => {
    const m = BrowserMediaDevices();
    await tick();
    await tick();
    assertDeepEq(m.devices.map((d) => [d.kind.$, d.label]), [[0, null], [1, null], [2, null]]);
  });
});

test("named formats try WebM, then MP4; only .exact can be unsupported", () => {
  const saved = globalThis.MediaRecorder;
  try {
    globalThis.MediaRecorder = { isTypeSupported: (t) => t.startsWith("audio/mp4") || t.startsWith("video/mp4") };
    assertEq(recordingType(unitCase(0))[0], "audio/mp4", "Safari records MP4");
    globalThis.MediaRecorder = { isTypeSupported: (t) => t.includes("webm") };
    assertDeepEq(recordingType(unitCase(0)), ["audio/webm;codecs=opus", 32000, undefined]);
    assertEq(recordingType(unitCase(2))[0], "video/webm;codecs=vp9,opus");
    assertEq(recordingType({ $: 3, mediaType: "audio/ogg", audioBitsPerSecond: null, videoBitsPerSecond: null }), null);
  } finally {
    globalThis.MediaRecorder = saved;
  }
});

test("a recorder records the stream's chunks into a File, and stops with the stream", async () => {
  const keys = ["MediaRecorder", "MediaStream", "Blob", "setInterval", "clearInterval", "performance"];
  const saved = keys.map((k) => globalThis[k]);
  let now = 0;
  const ticks = [];
  let recorder = null;
  try {
    globalThis.performance = { now: () => now };
    globalThis.setInterval = (f) => ticks.push(f);
    globalThis.clearInterval = () => {};
    globalThis.MediaStream = class {
      constructor(tracks) {
        this.tracks = tracks;
      }
    };
    globalThis.Blob = class {
      constructor(parts, o) {
        this.parts = parts;
        this.type = o.type;
      }
      async arrayBuffer() {
        return new Uint8Array(this.parts.flatMap((p) => p.bytes)).buffer;
      }
    };
    globalThis.MediaRecorder = class {
      static isTypeSupported = (t) => t.includes("webm");
      constructor(stream, o) {
        recorder = this;
        this.stream = stream;
        this.mimeType = o.mimeType;
        this.bits = o.audioBitsPerSecond;
        this.state = "inactive";
      }
      start() {
        this.state = "recording";
      }
      stop() {
        this.state = "inactive";
        queueMicrotask(() => this.onstop());
      }
    };
    const audio = mediaTrack(fakeTrack("audio"));
    const stream = mediaStream([audio], {});
    const r = BrowserRecorder();
    assertEq(thrown(() => r.start(mediaStream([], {}), unitCase(0), null)), unitCase(1), "noTrack");
    const session = r.start(stream, unitCase(0), { size: 16777216, duration: null });
    assertEq(recorder.bits, 32000, ".voice is 32 kbps");
    assertEq(recorder.stream.tracks.length, 1);
    recorder.ondataavailable({ data: { size: 3, bytes: [1, 2, 3] } });
    now = 2400;
    ticks[0]();
    assertEq(session.elapsed, 2000, "whole seconds while recording");
    assertEq(session.size, 3);
    audio.$t.onended();
    assertDeepEq(session.state, { $: 2, _0: unitCase(1) }, "the microphone went: streamEnded");
    const recording = await session.stop(null, false);
    assertEq(recording.duration, 2400);
    assertEq(recording.file.name, "recording.webm");
    assertEq(recording.file.mediaType, "audio/webm");
    assertDeepEq([...recording.file.bytes], [1, 2, 3]);
  } finally {
    keys.forEach((k, i) => (globalThis[k] = saved[i]));
  }
});

test("a controller drives one element, and follows its events", () => {
  const doc = installDom();
  const c = MediaController();
  const a = doc.createElement("audio");
  c.$attach(a);
  assertEq(c.state, unitCase(0));
  a.dispatchEvent(new Event("play"));
  assertEq(c.state, unitCase(2));
  a.currentTime = 1.5;
  a.dispatchEvent(new Event("timeupdate"));
  assertEq(c.currentTime, 1500);
  c.setVolume(2);
  assertEq(c.volume, 1, "clamped");
  assert(String(thrown(() => c.$attach(doc.createElement("video")))?.message).includes("drives one element"));
});

test("a local stream plays without its own sound unless monitored; a file gets an object URL", () => {
  const doc = installDom();
  const saved = [globalThis.URL, globalThis.Blob];
  const made = [];
  const revoked = [];
  globalThis.Blob = class {
    constructor(parts, o) {
      this.type = o.type;
    }
  };
  globalThis.URL = { createObjectURL: (b) => (made.push(b.type), `blob:${made.length}`), revokeObjectURL: (u) => revoked.push(u) };
  try {
    const stream = mediaStream([mediaTrack(fakeTrack("audio"))], { native: true });
    const el = doc.createElement("video");
    let dispose;
    root((d) => {
      dispose = d;
      mediaElement(el, () => ({ $: 0, _0: stream }), {});
    });
    flush();
    assert(el.muted, "a local stream is muted");
    assert(!el.controls, "no controls for a stream");
    assertEq(el.srcObject, stream.$s);
    dispose();
    const el2 = doc.createElement("audio");
    root((d) => {
      dispose = d;
      mediaElement(el2, () => ({ $: 0, _0: stream }), { monitor: () => true });
    });
    flush();
    assert(!el2.muted, "monitored");
    dispose();
    const el3 = doc.createElement("audio");
    root((d) => {
      dispose = d;
      mediaElement(el3, () => ({ $: 1, _0: { name: "m.webm", mediaType: "audio/webm", bytes: new Uint8Array([1]) } }), {});
    });
    flush();
    assertEq(el3.getAttribute("src"), "blob:1");
    assertDeepEq(made, ["audio/webm"]);
    assert(el3.controls, "controls for a file");
    dispose();
    assertDeepEq(revoked, ["blob:1"], "revoked when the element unmounts");
  } finally {
    [globalThis.URL, globalThis.Blob] = saved;
  }
});
