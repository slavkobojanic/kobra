/**
 * @module k:runtime/media
 *
 * `k.media` in the browser (docs/design/46-media-and-rtc.md;
 * docs/compiler/runtime.md §12.14): `BrowserMediaDevices` over
 * `getUserMedia`, `getDisplayMedia` and `enumerateDevices`,
 * `BrowserRecorder` over `MediaRecorder`, `BrowserAudio` over one
 * `AudioContext`, `MediaController` and the `LevelMeter` component.
 *
 * Streams, tracks, sessions and meters are handles: objects whose getters
 * read signals, so views that read them update, and whose `equals` is
 * identity. The enums are the stdlib's cases: `PermissionState`
 * `unitCase(0)` `prompt`, `1` `granted`, `2` `denied`; `CaptureError` `0`
 * `unsupported`, `1` `notAsked`, `2` `denied`, `3` `cancelled`, `4`
 * `notFound`, `5` `busy`, `{$: 6, setting}` `unsatisfiable`;
 * `CaptureLifetime` `0` `owner`, `1` `service`; `TrackKind` `0` `audio`,
 * `1` `video`; `TrackState` `0` `live`, `{$: 1, _0: TrackEnd}` `ended`;
 * `TrackEnd` `0` `stopped`, `1` `deviceLost`, `2` `endedByUser`, `3`
 * `remoteEnded`; `RecordingFormat` `0` `voice`, `1` `audio`, `2` `video`,
 * `3` `exact`; `RecordingState` `0` `recording`, `1` `paused`, `{$: 2,
 * _0: RecordingStop}` `stopped`; `RecordingStop` `0` `byApp`, `1`
 * `streamEnded`, `2` `limitReached`; `RecordingError` `0`
 * `unsupportedFormat`, `1` `noTrack`, `2` `failed`; `PlaybackState` `0`
 * `idle` … `5` `blocked`; `PlaybackError` `0` `blocked`, `1`
 * `unsupportedFormat`, `2` `unreachable`; `AudioError` `0` `unsupported`,
 * `1` `noTrack`. Durations are milliseconds.
 *
 * Shipped as its own client chunk, only to pages whose client uses
 * `k.media` (codegen.md §16), and measured against its own size budget.
 * The `video` and `audio` elements' binding is the playback chunk's
 * (`playback.js`), which drives a `MediaController` through `$attach`.
 */

import { CANCELLED, panic, unitCase } from "./values.js";
import { batch, onCleanup, renderEffect, signal } from "./signals.js";
import { claimRaw } from "./hydrate.js";

/** `PermissionState` by the Permissions API's state name. */
const MEDIA_STATES = { prompt: 0, granted: 1, denied: 2 };

/** A `TrackState.ended(end)`. */
const trackEnded = (end) => ({ $: 1, _0: unitCase(end) });

/** The browser's media devices, or `undefined` outside a secure context. */
const mediaApi = () => (globalThis.isSecureContext === true ? globalThis.navigator?.mediaDevices : undefined);

/**
 * A `MediaTrack` handle of a browser `MediaStreamTrack` (or of the test
 * kit's synthetic track, `t` with `stop()`): every field a signal. It
 * ends with `.deviceLost` when the browser ends it (`.endedByUser` for a
 * screen), and with `.stopped` when stopped. `ends` are run once when it
 * ends.
 * @param {object} t the browser's track
 * @param {{screen?: boolean, device?: string|null, settings?: object}} [o]
 */
export function mediaTrack(t, o = {}) {
  const enabled = signal(t.enabled !== false);
  const muted = signal(t.muted === true);
  const state = signal(unitCase(0));
  const ends = new Set();
  const end = (why) => {
    if (state.peek().$ !== 0) return;
    batch(() => state.set(trackEnded(why)));
    for (const f of [...ends]) f(why);
  };
  t.onmute = () => muted.set(true);
  t.onunmute = () => muted.set(false);
  t.onended = () => end(o.screen ? 2 : 1);
  const s = t.getSettings?.() ?? {};
  const known = (v) => v ?? null;
  const id = o.device ?? (o.screen ? null : s.deviceId || null);
  return {
    kind: unitCase(t.kind === "video" ? 1 : 0),
    label: String(t.label ?? ""),
    device: id === null ? null : { value: id },
    settings: o.settings ?? {
      size: s.width ? { width: s.width, height: s.height } : null,
      frameRate: known(s.frameRate),
      sampleRate: known(s.sampleRate),
      channelCount: known(s.channelCount),
      echoCancellation: known(s.echoCancellation),
      noiseSuppression: known(s.noiseSuppression),
      autoGainControl: known(s.autoGainControl),
    },
    get isEnabled() {
      return enabled.get();
    },
    get isMuted() {
      return muted.get();
    },
    get state() {
      return state.get();
    },
    setEnabled(v) {
      t.enabled = v === true;
      enabled.set(v === true);
    },
    stop() {
      t.stop();
      end(0);
    },
    equals(b) {
      return this === b;
    },
    $t: t,
    $end: end,
    $ends: ends,
  };
}

/**
 * A `MediaStream` handle of track handles; `$s` is the browser's stream
 * (`null` in tests). `isLive` reads every track's state.
 * @param {Array<object>} tracks
 * @param {object|null} native
 * @param {boolean} [local]
 */
export function mediaStream(tracks, native, local = true) {
  const of = (k) => tracks.filter((t) => t.kind.$ === k);
  return {
    get audioTracks() {
      return of(0);
    },
    get videoTracks() {
      return of(1);
    },
    get isLive() {
      return tracks.some((t) => t.state.$ === 0);
    },
    isLocal: local,
    stop() {
      batch(() => tracks.forEach((t) => t.stop()));
    },
    equals(b) {
      return this === b;
    },
    $s: native,
    $tracks: tracks,
  };
}

/** The streams captured with `CaptureLifetime.service` on this page. */
const kept = new Set();

/**
 * Ties `stream` to its owner (`CaptureLifetime`): `.owner` stops it when
 * `c`, the run's cancellation signal, aborts; `.service` when the page
 * that captured it is left (another path, or the document unloads). A
 * stream whose owner is already gone is stopped at once, and the call
 * stops with `CANCELLED` before K code sees it.
 * @param {object} stream
 * @param {AbortSignal|null} c
 * @param {{$: number}|null} lifetime
 */
export function ownStream(stream, c, lifetime) {
  if (c?.aborted) {
    stream.stop();
    throw CANCELLED;
  }
  if (lifetime?.$ === 1) {
    keepForPage(stream);
  } else {
    c?.addEventListener("abort", () => stream.stop());
  }
  return stream;
}

/** The page's path when the `.service` streams were captured. */
let keptAt = null;

/** Stops the `.service` streams when the page is left. */
function leaveCheck() {
  if (globalThis.location?.pathname !== keptAt) {
    for (const s of [...kept]) s.stop();
    kept.clear();
  }
}

/** Keeps `stream` until the page is left, watching the history once. */
function keepForPage(stream) {
  const h = globalThis.history;
  if (keptAt === null && h) {
    for (const k of ["pushState", "replaceState"]) {
      const f = h[k];
      if (typeof f === "function") {
        h[k] = function (...a) {
          const r = f.apply(this, a);
          leaveCheck();
          return r;
        };
      }
    }
    globalThis.addEventListener?.("popstate", leaveCheck);
    globalThis.addEventListener?.("pagehide", () => {
      keptAt = "";
      leaveCheck();
    });
  }
  keptAt = globalThis.location?.pathname ?? "";
  kept.add(stream);
}

/** The `getUserMedia` mediaConstraints of a `CaptureRequest`. */
function mediaConstraints(r) {
  const audio = r.$ === 0 ? r._0 : r.$ === 2 ? r._0 : null;
  const video = r.$ === 1 ? r._0 : r.$ === 2 ? r._1 : null;
  const exact = (d) => (d ? { deviceId: { exact: d.value } } : {});
  const ideal = (v) => (v === null || v === undefined ? undefined : { ideal: v });
  return {
    audio: audio && { ...exact(audio.device), echoCancellation: audio.echoCancellation, noiseSuppression: audio.noiseSuppression, autoGainControl: audio.autoGainControl },
    video: video && {
      ...exact(video.device),
      facingMode: ideal(video.facing && ["user", "environment"][video.facing.$]),
      width: ideal(video.size?.width),
      height: ideal(video.size?.height),
      frameRate: ideal(video.frameRate),
    },
  };
}

/** The kinds a request asks for: `[camera?, microphone?]` as permission names. */
const captureKinds = (r) => (r.$ === 0 ? ["microphone"] : r.$ === 1 ? ["camera"] : ["camera", "microphone"]);

/**
 * A browser capture error as a `CaptureError`: `NotAllowedError` is
 * `.denied` (`.cancelled` from the screen picker), a missing chosen
 * device `.notFound`, a device another application holds `.busy`.
 * @param {{name?: string, constraint?: string}} e
 * @param {boolean} screen
 */
export function captureError(e, screen) {
  switch (e?.name) {
    case "NotAllowedError":
      return unitCase(screen ? 3 : 2);
    case "NotFoundError":
      return unitCase(4);
    case "NotReadableError":
    case "AbortError":
      return unitCase(5);
    case "OverconstrainedError":
      return e.constraint === "deviceId" ? unitCase(4) : { $: 6, setting: String(e.constraint ?? "") };
    default:
      return unitCase(0);
  }
}

/**
 * A `MediaDevice` of a browser `MediaDeviceInfo`; `null` for a kind K
 * doesn't list.
 * @param {MediaDeviceInfo} d
 */
const mediaDevice = (d) => {
  const k = ["audioinput", "videoinput", "audiooutput"].indexOf(d.kind);
  return k < 0 ? null : { id: { value: String(d.deviceId) }, kind: unitCase(k), label: d.label ? String(d.label) : null };
};

/**
 * The browser's `k.media.BrowserMediaDevices`. The two permissions are
 * signals, `.prompt` until the Permissions API answers (and where it
 * doesn't know the name, as Gecko for `camera`), kept current by its
 * `change` events and re-read when the page becomes visible; a grant seen
 * by a capture counts for the page's life, as for `Geolocation`.
 * `devices` is enumerated after hydration, on `devicechange` and after a
 * grant.
 *
 * `requestAccess` opens the devices, which is what prompts, and keeps the
 * stream for 5 seconds: a `capture` of the same request in that time
 * takes it over. `capture` reads the permissions first and never prompts.
 * @returns {object}
 */
export function BrowserMediaDevices() {
  const perms = { camera: signal(unitCase(0)), microphone: signal(unitCase(0)) };
  const granted = { camera: false, microphone: false };
  const status = {};
  const list = signal([]);
  let held = null;
  const query = async (name) => {
    try {
      status[name] ??= await navigator.permissions.query({ name });
      const s = MEDIA_STATES[status[name].state] ?? 0;
      status[name].onchange ??= () => query(name);
      perms[name].set(unitCase(s === 0 && granted[name] ? 1 : s));
    } catch {
      // No Permissions API, or not this name in it: keep what is known.
    }
    return perms[name].peek().$;
  };
  const enumerate = async () => {
    try {
      const all = await mediaApi().enumerateDevices();
      list.set(all.map(mediaDevice).filter((d) => d !== null));
    } catch {
      // Enumeration refused: keep the list.
    }
  };
  if (mediaApi()) {
    queueMicrotask(() => {
      query("camera");
      query("microphone");
      enumerate();
    });
    mediaApi().addEventListener?.("devicechange", enumerate);
    globalThis.document?.addEventListener?.("visibilitychange", () => document.visibilityState === "visible" && (query("camera"), query("microphone")));
  }
  const grant = (names) => {
    for (const n of names) (granted[n] = true), perms[n].set(unitCase(1));
    enumerate();
  };
  /** Opens a stream for `request`, or rejects with its `CaptureError`. */
  const open = async (request) => {
    const key = JSON.stringify(mediaConstraints(request));
    if (held?.key === key) {
      const s = held.stream;
      clearTimeout(held.timer);
      held = null;
      return s;
    }
    return mediaApi().getUserMedia(mediaConstraints(request));
  };
  const handle = (native, screen) =>
    mediaStream(
      native.getTracks().map((t) => mediaTrack(t, { screen })),
      native,
    );
  return {
    get isSupported() {
      return typeof mediaApi()?.getUserMedia === "function";
    },
    get cameraPermission() {
      return perms.camera.get();
    },
    get microphonePermission() {
      return perms.microphone.get();
    },
    get devices() {
      return list.get();
    },
    async requestAccess(_c, request) {
      const names = captureKinds(request);
      const now = () => {
        const s = names.map((n) => perms[n].peek().$);
        return unitCase(s.includes(2) ? 2 : s.every((x) => x === 1) ? 1 : 0);
      };
      if (!mediaApi()) return now();
      for (const n of names) await query(n);
      if (now().$ !== 0) return now();
      try {
        const stream = await mediaApi().getUserMedia(mediaConstraints(request));
        if (held) held.stream.getTracks().forEach((t) => t.stop());
        const key = JSON.stringify(mediaConstraints(request));
        held = { key, stream, timer: setTimeout(() => held?.stream === stream && (stream.getTracks().forEach((t) => t.stop()), (held = null)), 5000) };
        grant(names);
      } catch (e) {
        // Refused (`denied`), or dismissed: the permission still says
        // `prompt`. Any other failure came after the grant.
        for (const n of names) await query(n);
        if (e?.name !== "NotAllowedError") grant(names.filter((n) => perms[n].peek().$ === 0));
      }
      return now();
    },
    async capture(c, request, lifetime) {
      if (!mediaApi()) throw unitCase(0);
      for (const n of captureKinds(request)) {
        const s = await query(n);
        if (s !== 1) throw unitCase(s === 0 ? 1 : 2);
      }
      let native;
      try {
        native = await open(request);
      } catch (e) {
        throw captureError(e, false);
      }
      grant(captureKinds(request));
      return ownStream(handle(native, false), c, lifetime);
    },
    async captureScreen(c, request, lifetime) {
      if (typeof mediaApi()?.getDisplayMedia !== "function") throw unitCase(0);
      let native;
      try {
        const surface = request?.surface ? ["browser", "window", "monitor"][request.surface.$] : undefined;
        native = await mediaApi().getDisplayMedia({ video: surface ? { displaySurface: surface } : true, audio: request?.audio === true });
      } catch (e) {
        throw captureError(e, true);
      }
      return ownStream(handle(native, true), c, lifetime);
    },
  };
}

/** `[WebM type, MP4 type, audio bps, video bps]` of the named formats. */
const FORMATS = [
  ["audio/webm;codecs=opus", "audio/mp4", 32000, undefined],
  ["audio/webm;codecs=opus", "audio/mp4", 128000, undefined],
  ["video/webm;codecs=vp9,opus", "video/mp4", 128000, 2500000],
];

/**
 * The media type and bit rates `format` records with here: the WebM form,
 * else the MP4 form (`MediaRecorder.isTypeSupported`), else the
 * browser's own choice; `null` for an `.exact` type it can't record.
 * @param {{$: number}} f
 */
export function recordingType(f) {
  const ok = (t) => globalThis.MediaRecorder?.isTypeSupported?.(t) === true;
  if (f.$ === 3) return ok(f.mediaType) ? [f.mediaType, f.audioBitsPerSecond ?? undefined, f.videoBitsPerSecond ?? undefined] : null;
  const [webm, mp4, a, v] = FORMATS[f.$];
  const alt = f.$ === 2 && !ok(webm) && ok("video/webm;codecs=vp8,opus") ? "video/webm;codecs=vp8,opus" : null;
  return [ok(webm) ? webm : alt ?? (ok(mp4) ? mp4 : ""), a, v];
}

/**
 * A `RecordingSession` handle over a recorder `rec` (the browser's
 * `MediaRecorder`, or the test kit's): `state`, `elapsed` and `size` are
 * signals; `elapsed` counts on `now()` (milliseconds), paused time
 * excluded, and is set each second by `tick`. It stops by itself when the
 * stream ends or at its limit. `rec` has `start()`, `pause()`,
 * `resume()` and `stop()`, which resolves with the recorded bytes and
 * media type.
 * @param {object} stream the `MediaStream` handle
 * @param {object} format the `RecordingFormat`
 * @param {{size: number, duration: number|null}} limit
 * @param {object} rec
 * @param {() => number} now
 * @param {(f: () => void) => {cancel(): void}} every runs `f` each second
 */
export function recordingSession(stream, format, limit, rec, now, every) {
  // A call through the interface leaves a default argument out.
  limit ??= { size: 16777216, duration: null };
  const state = signal(unitCase(0));
  const elapsed = signal(0);
  const size = signal(0);
  let base = 0;
  let since = now();
  let result = null;
  const time = () => base + (state.peek().$ === 0 ? now() - since : 0);
  const tick = () => {
    elapsed.set(Math.floor(time() / 1000) * 1000);
    if (limit.duration !== null && limit.duration !== undefined && time() >= limit.duration) finish(2);
  };
  const timer = every(tick);
  const finish = (why) => {
    if (result !== null) return result;
    const duration = limit.duration !== null && limit.duration !== undefined ? Math.min(time(), limit.duration) : time();
    timer.cancel();
    batch(() => {
      elapsed.set(duration);
      state.set({ $: 2, _0: unitCase(why) });
    });
    result = Promise.resolve(rec.stop()).then(({ bytes, mediaType }) => {
      const ext = mediaType.includes("mp4") ? "mp4" : "webm";
      return { file: { name: `recording.${ext}`, mediaType, bytes }, duration, format };
    });
    return result;
  };
  rec.data = (n) => {
    size.set(size.peek() + n);
    if (size.peek() >= limit.size) finish(2);
  };
  for (const t of stream.$tracks) t.$ends.add(() => finish(1));
  rec.start();
  return {
    get state() {
      return state.get();
    },
    get elapsed() {
      return elapsed.get();
    },
    get size() {
      return size.get();
    },
    pause() {
      if (state.peek().$ !== 0) return;
      base = time();
      rec.pause();
      state.set(unitCase(1));
    },
    resume() {
      if (state.peek().$ !== 1) return;
      since = now();
      rec.resume();
      state.set(unitCase(0));
    },
    async stop(_c, keepStream) {
      const r = finish(0);
      if (keepStream !== true) stream.stop();
      try {
        return await r;
      } catch (e) {
        throw e === CANCELLED ? e : unitCase(2);
      }
    },
  };
}

/** The tracks a format records, or `null` when the stream lacks one. */
export function recordedTracks(stream, format) {
  const audio = stream.audioTracks;
  const video = stream.videoTracks;
  const isVideo = format.$ === 2 || (format.$ === 3 && format.mediaType.startsWith("video/"));
  if (isVideo ? video.length === 0 : audio.length === 0) return null;
  return isVideo ? [...video, ...audio] : audio;
}

/**
 * The browser's `k.media.BrowserRecorder`: a `MediaRecorder` of the
 * tracks the format records, emitting data each second so `size` follows
 * it.
 * @returns {object}
 */
export function BrowserRecorder() {
  return {
    supports(format) {
      return format.$ !== 3 || recordingType(format) !== null;
    },
    start(stream, format, limit) {
      const tracks = recordedTracks(stream, format);
      if (tracks === null) throw unitCase(1);
      const type = recordingType(format);
      if (type === null || typeof globalThis.MediaRecorder !== "function") throw unitCase(0);
      const [mimeType, audioBitsPerSecond, videoBitsPerSecond] = type;
      const mr = new MediaRecorder(new MediaStream(tracks.map((t) => t.$t)), { mimeType: mimeType || undefined, audioBitsPerSecond, videoBitsPerSecond });
      const chunks = [];
      const rec = {
        start: () => mr.start(1000),
        pause: () => mr.pause(),
        resume: () => mr.resume(),
        stop: () =>
          new Promise((resolve, reject) => {
            const done = async () => {
              const blob = new Blob(chunks, { type: mr.mimeType || mimeType });
              resolve({ bytes: new Uint8Array(await blob.arrayBuffer()), mediaType: (blob.type || mimeType).split(";")[0] });
            };
            if (mr.state === "inactive") return done();
            mr.onstop = done;
            mr.onerror = () => reject(unitCase(2));
            mr.stop();
          }),
      };
      mr.ondataavailable = (e) => {
        if (e.data?.size) {
          chunks.push(e.data);
          rec.data(e.data.size);
        }
      };
      return recordingSession(stream, format, limit, rec, () => performance.now(), (f) => {
        const id = setInterval(f, 1000);
        return { cancel: () => clearInterval(id) };
      });
    },
  };
}

/**
 * An `AudioMeter` handle reading `sample()`, the loudness now (RMS,
 * `0…1`), at most 30 times a second on `schedule`: `level` on a
 * perceptual scale (-60 dB to 0 dB), `peak` the highest of the last 1.5
 * seconds, `isSpeaking` after 150 ms above 0.3. `now()` is in
 * milliseconds; `wave(n)` the waveform snapshot. Stops with its stream.
 * @param {object} stream
 * @param {() => number} sample
 * @param {(n: number) => number[]} wave
 * @param {() => number} now
 * @param {(f: () => void) => () => void} schedule runs `f` repeatedly; returns a stop
 */
export function audioMeter(stream, sample, wave, now, schedule) {
  const level = signal(0);
  const peak = signal(0);
  const speaking = signal(false);
  let peaks = [];
  let loudSince = null;
  let last = -Infinity;
  const read = () => {
    const t = now();
    if (t - last < 1000 / 30) return;
    last = t;
    const rms = sample();
    const l = rms <= 0 ? 0 : Math.round(Math.max(0, Math.min(1, (20 * Math.log10(rms) + 60) / 60)) * 1000) / 1000;
    peaks = peaks.filter(([at]) => t - at < 1500);
    peaks.push([t, l]);
    if (l > 0.3) loudSince ??= t;
    else loudSince = null;
    batch(() => {
      level.set(l);
      peak.set(Math.max(...peaks.map(([, p]) => p)));
      speaking.set(loudSince !== null && t - loudSince >= 150);
    });
  };
  let cancel = schedule(read);
  const stop = () => {
    cancel?.();
    cancel = null;
  };
  for (const t of stream.$tracks) t.$ends.add(stop);
  return {
    get level() {
      return level.get();
    },
    get peak() {
      return peak.get();
    },
    get isSpeaking() {
      return speaking.get();
    },
    waveform(n) {
      return wave(n);
    },
    stop,
    $read: read,
  };
}

/** The app's one `AudioContext`, made on first use. */
let sharedContext = null;

/** The shared context, resumed on the page's first user action. */
function audioContext() {
  if (sharedContext === null && typeof globalThis.AudioContext === "function") {
    sharedContext = new AudioContext();
    const resume = () => sharedContext.state === "suspended" && sharedContext.resume();
    for (const e of ["pointerdown", "keydown"]) globalThis.document?.addEventListener(e, resume, { once: true, capture: true });
  }
  return sharedContext;
}

/**
 * The browser's `k.media.BrowserAudio`: meters read an `AnalyserNode` on
 * the shared context each animation frame; `play` plays a source through
 * an element of its own and resolves when it ends.
 * @returns {object}
 */
export function BrowserAudio() {
  return {
    meter(stream, lifetime) {
      const ctx = audioContext();
      if (ctx === null || !stream.$s) throw unitCase(0);
      if (stream.audioTracks.length === 0) throw unitCase(1);
      const node = ctx.createAnalyser();
      node.fftSize = 2048;
      const source = ctx.createMediaStreamSource(stream.$s);
      source.connect(node);
      const data = new Float32Array(node.fftSize);
      const sample = () => {
        node.getFloatTimeDomainData(data);
        let sum = 0;
        for (const v of data) sum += v * v;
        return Math.sqrt(sum / data.length);
      };
      const wave = (n) => {
        node.getFloatTimeDomainData(data);
        const span = Math.min(data.length, Math.round(ctx.sampleRate * 0.05));
        const step = span / Math.max(1, n);
        return Array.from({ length: Math.max(0, n) }, (_, i) => data[data.length - span + Math.floor(i * step)] ?? 0);
      };
      const m = audioMeter(stream, sample, wave, () => performance.now(), (f) => {
        let id = requestAnimationFrame(function frame() {
          f();
          id = requestAnimationFrame(frame);
        });
        return () => {
          cancelAnimationFrame(id);
          source.disconnect();
        };
      });
      if (lifetime?.$ !== 1) onCleanup(m.stop);
      return m;
    },
    async play(c, source) {
      const a = document.createElement("audio");
      const url = source.$ === 1 ? URL.createObjectURL(new Blob([source._0.bytes], { type: source._0.mediaType })) : null;
      if (source.$ === 0) a.srcObject = source._0.$s;
      else a.src = url ?? source._0;
      try {
        await new Promise((resolve, reject) => {
          a.onended = resolve;
          a.onerror = () => reject(unitCase(2));
          c?.addEventListener("abort", () => (a.pause(), resolve()));
          a.play().catch((e) => reject(playbackError(e)));
        });
      } finally {
        if (url) URL.revokeObjectURL(url);
      }
    },
  };
}

/** A rejected `play()` as a `PlaybackError`. */
export const playbackError = (e) => unitCase(e?.name === "NotAllowedError" ? 0 : e?.name === "NotSupportedError" ? 1 : 2);

/**
 * `new MediaController()`: the state of the element it is given with
 * `.controller(…)` (`$attach`, by the playback chunk), in signals that
 * follow the element's events. A second element is a panic naming both.
 * @returns {object}
 */
export function MediaController() {
  const state = signal(unitCase(0));
  const time = signal(0);
  const length = signal(null);
  const volume = signal(1);
  let el = null;
  const name = (e) => `<${e.tagName?.toLowerCase()}${e.getAttribute?.("aria-label") ? ` "${e.getAttribute("aria-label")}"` : ""}>`;
  const events = {
    loadstart: () => state.set(unitCase(1)),
    play: () => state.set(unitCase(2)),
    playing: () => state.set(unitCase(2)),
    pause: () => el?.ended || state.set(unitCase(3)),
    ended: () => state.set(unitCase(4)),
    timeupdate: () => time.set(Math.round((el?.currentTime ?? 0) * 1000)),
    durationchange: () => length.set(Number.isFinite(el?.duration) ? Math.round(el.duration * 1000) : null),
    volumechange: () => volume.set(el?.volume ?? 1),
  };
  const c = {
    get state() {
      return state.get();
    },
    get currentTime() {
      return time.get();
    },
    get duration() {
      return length.get();
    },
    get volume() {
      return volume.get();
    },
    async play(_c) {
      if (el === null) return;
      try {
        await el.play();
      } catch (e) {
        const error = playbackError(e);
        if (error.$ === 0) state.set(unitCase(5));
        throw error;
      }
    },
    pause() {
      el?.pause();
    },
    async toggle(_c) {
      if (state.peek().$ === 2) return el?.pause();
      try {
        await c.play(null);
      } catch (e) {
        if (e === CANCELLED) throw e;
      }
    },
    seek(ms) {
      if (el !== null) el.currentTime = ms / 1000;
      time.set(ms);
    },
    setVolume(v) {
      const x = Math.max(0, Math.min(1, v));
      if (el !== null) el.volume = x;
      volume.set(x);
    },
    $attach(e) {
      if (el !== null && el !== e) panic(`a MediaController drives one element: ${name(el)} and ${name(e)}`);
      el = e;
      for (const k in events) e.addEventListener(k, events[k]);
      e.volume = volume.peek();
    },
    $detach(e) {
      if (el !== e) return;
      for (const k in events) e.removeEventListener(k, events[k]);
      el = null;
    },
  };
  return c;
}

/**
 * The browser's `LevelMeter`: a `role="meter"` bar of `meter.level`, with
 * a mark at `peak`; `aria-valuenow` (0–100) changes at most once a second
 * on the container's `Clock`, so a screen reader isn't flooded. Takes the
 * server's element while hydrating.
 * @param {Record<string, Function>} props `meter`, `label`
 * @param {{c: object}} ctx the render context, whose container has `Clock`
 * @returns {Element}
 */
export function LevelMeter(props, ctx) {
  const clock = ctx?.c?.Clock;
  const el = claimRaw("div") ?? document.createElement("div");
  while (el.firstChild) el.removeChild(el.firstChild);
  el.setAttribute("class", "k-level-meter");
  el.setAttribute("role", "meter");
  el.setAttribute("aria-valuemin", "0");
  el.setAttribute("aria-valuemax", "100");
  el.setAttribute("style", "position: relative; height: 0.5rem; background: color-mix(in srgb, currentColor 15%, transparent); border-radius: 0.25rem; overflow: hidden");
  const bar = el.appendChild(document.createElement("div"));
  const mark = el.appendChild(document.createElement("div"));
  const now = () => clock?.now.ms ?? 0;
  let said = -Infinity;
  let timer = null;
  let value = 0;
  const say = () => {
    said = now();
    timer = null;
    el.setAttribute("aria-valuenow", String(value));
  };
  renderEffect(() => el.setAttribute("aria-label", String(props.label())));
  renderEffect(() => {
    const m = props.meter();
    value = Math.round(m.level * 100);
    const peak = Math.round(m.peak * 100);
    bar.setAttribute("style", `position: absolute; inset: 0 auto 0 0; width: ${value}%; background: currentColor`);
    mark.setAttribute("style", `position: absolute; top: 0; bottom: 0; left: ${peak}%; width: 2px; background: currentColor`);
    const wait = 1000 - (now() - said);
    if (wait <= 0 || !clock) say();
    else timer ??= clock.after(wait, say);
  });
  onCleanup(() => timer?.cancel());
  return el;
}
