/**
 * @module k:runtime/files
 *
 * `k.device.BrowserFiles` (docs/design/42-files.md; docs/compiler/runtime.md
 * §12.4): opening files the user chooses with the browser's picker (a
 * hidden `<input type="file">`), read whole into `Bytes`, and saving a
 * `File` as a download through an object URL.
 *
 * The errors are the stdlib enums' cases, which need no generated code:
 * a payload-free case is the shared `unitCase(tag)`, and `tooLarge` is
 * `{$: 1, limit}`.
 *
 * Shipped as its own client chunk, only to apps whose client injects
 * `Files` (codegen.md §16), and measured against its own size budget.
 */

import { unitCase } from "./values.js";

/** `File.maximumSize`: the largest file `open` reads, in bytes. */
const LIMIT = 16777216;

/** `[media type, extension]` of `FileType`'s cases without a payload, by tag. */
const TYPES = [
  ["application/json", "json"],
  ["text/plain", "txt"],
  ["text/csv", "csv"],
  ["text/markdown", "md"],
  ["image/png", "png"],
  ["image/jpeg", "jpg"],
  ["image/svg+xml", "svg"],
  ["application/pdf", "pdf"],
];

/**
 * The `accept` attribute of a list of `FileType` values: each one's media
 * type and `.extension`.
 * @param {Array<{$: number, mediaType?: string, fileExtension?: string}>} types
 * @returns {string}
 */
export function acceptOf(types) {
  return types
    .flatMap((t) => {
      const [m, e] = t.$ === 8 ? [t.mediaType, t.fileExtension] : TYPES[t.$] ?? [];
      return m ? [m, `.${e}`] : [];
    })
    .join(",");
}

/**
 * Reads a browser `File` into a K `File`: `{name, mediaType, bytes}`.
 * @param {Blob & {name: string}} f
 */
async function readFile(f) {
  return { name: String(f.name ?? ""), mediaType: String(f.type ?? ""), bytes: new Uint8Array(await f.arrayBuffer()) };
}

/**
 * Shows the picker for `types` and resolves with the chosen files, read;
 * rejects with `FileOpenError`'s `.cancelled`, `.tooLarge` or `.unreadable`.
 * It must run in a user's action, before its first `await`: browsers open
 * a picker only then.
 * @param {Array<object>} types
 * @param {boolean} multiple
 * @returns {Promise<Array<object>>}
 */
function pick(types, multiple) {
  return new Promise((resolve, reject) => {
    const input = document.createElement("input");
    input.setAttribute("type", "file");
    input.setAttribute("style", "display: none");
    const accept = acceptOf(types);
    if (accept) input.setAttribute("accept", accept);
    if (multiple) input.setAttribute("multiple", "");
    document.body.appendChild(input);
    const fail = (tag) => {
      input.remove();
      reject(unitCase(tag));
    };
    input.addEventListener("cancel", () => fail(0));
    input.addEventListener("change", () => {
      input.remove();
      const chosen = [...(input.files ?? [])];
      if (chosen.length === 0) return reject(unitCase(0));
      if (chosen.some((f) => f.size > LIMIT)) return reject({ $: 1, limit: LIMIT });
      Promise.all(chosen.map(readFile)).then(resolve, () => reject(unitCase(2)));
    });
    try {
      input.click();
    } catch {
      fail(2);
    }
  });
}

/**
 * The browser's `k.device.BrowserFiles`: `open` and `openAll` (async, so
 * their first argument is the call's cancellation signal) and `save`,
 * which throws `FileSaveError.unavailable` when the browser refuses.
 * @returns {{open: Function, openAll: Function, save: Function}}
 */
export function BrowserFiles() {
  return {
    open: async (_c, types) => (await pick(types, false))[0],
    openAll: (_c, types) => pick(types, true),
    save(file) {
      try {
        const url = URL.createObjectURL(new Blob([file.bytes], { type: file.mediaType || "application/octet-stream" }));
        const a = document.createElement("a");
        a.setAttribute("href", url);
        a.setAttribute("download", file.name);
        a.setAttribute("style", "display: none");
        document.body.appendChild(a);
        a.click();
        a.remove();
        // The download has the blob once it started; free it a little later.
        globalThis.setTimeout?.(() => URL.revokeObjectURL(url), 10000);
      } catch {
        throw unitCase(0);
      }
    },
  };
}
