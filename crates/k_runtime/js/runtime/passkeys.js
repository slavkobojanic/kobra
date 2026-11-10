/**
 * @module k:runtime/passkeys
 *
 * `k.auth.BrowserPasskeys` in the browser (docs/design/51-passkeys.md;
 * docs/compiler/runtime.md §12.17): WebAuthn's `navigator.credentials`
 * with a public-key credential. The server's options arrive as the
 * stdlib's structs of base64url text, and the results leave the same way,
 * so this chunk only moves bytes: every check is the server's.
 *
 * `PasskeyError` is `unitCase(0)` `unsupported`, `1` `cancelled`, `2`
 * `alreadyRegistered`, `3` `failed`.
 *
 * Shipped as its own client chunk, only to apps whose client injects
 * `Passkeys` (codegen.md §16), and measured against its own size budget.
 */

import { unitCase } from "./values.js";

/** The base64url alphabet, by digit value. */
const PASSKEY_B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/** Base64url text as bytes (the server wrote it, so it is well formed). */
const passkeyBytes = (t) => {
  const out = [];
  let v = 0;
  let n = 0;
  for (const c of String(t)) {
    v = ((v << 6) | PASSKEY_B64.indexOf(c)) & 0xffffff;
    n += 6;
    if (n >= 8) out.push((v >> (n -= 8)) & 255);
  }
  return Uint8Array.from(out);
};

/** An `ArrayBuffer`'s bytes as unpadded base64url. */
const passkeyText = (buffer) => {
  const b = new Uint8Array(buffer);
  let out = "";
  for (let i = 0; i < b.length; i += 3) {
    const v = (b[i] << 16) | ((b[i + 1] ?? 0) << 8) | (b[i + 2] ?? 0);
    out += PASSKEY_B64[v >> 18] + PASSKEY_B64[(v >> 12) & 63];
    if (i + 1 < b.length) out += PASSKEY_B64[(v >> 6) & 63];
    if (i + 2 < b.length) out += PASSKEY_B64[v & 63];
  }
  return out;
};

/** Credential descriptors for `excludeCredentials` and `allowCredentials`. */
const passkeyCredentials = (ids) => (ids ?? []).map((id) => ({ type: "public-key", id: passkeyBytes(id) }));

/**
 * The `PasskeyError` for a WebAuthn rejection: browsers report a dismissed
 * sheet, a timeout and "no passkey here" alike, as `NotAllowedError`.
 */
const passkeyFailure = (e) => unitCase(e?.name === "NotAllowedError" || e?.name === "AbortError" ? 1 : e?.name === "InvalidStateError" ? 2 : 3);

/** Five minutes, WebAuthn's recommended ceremony timeout. */
const PASSKEY_TIMEOUT = 300000;

/**
 * The browser's `k.auth.BrowserPasskeys`. `isSupported` is constant for
 * the page's life. Each ceremony asks for a discoverable credential with
 * user verification and attestation `"none"`, offering EdDSA, ES256 and
 * RS256.
 * @returns {object}
 */
export function BrowserPasskeys() {
  const g = globalThis;
  const api = () => (g.isSecureContext === true && typeof g.PublicKeyCredential === "function" ? g.navigator?.credentials : undefined);
  /** Runs one ceremony, mapping its rejection to a `PasskeyError`. */
  const run = async (call) => {
    const c = api();
    if (!c) throw unitCase(0);
    try {
      return await call(c);
    } catch (e) {
      throw passkeyFailure(e);
    }
  };
  return {
    get isSupported() {
      return api() !== undefined;
    },
    register(_c, o, name) {
      return run(async (c) => {
        const r = await c.create({
          publicKey: {
            challenge: passkeyBytes(o.challenge),
            rp: { id: o.rpId, name: o.rpName },
            user: { id: passkeyBytes(o.userHandle), name: String(name), displayName: String(name) },
            pubKeyCredParams: [-8, -7, -257].map((alg) => ({ type: "public-key", alg })),
            excludeCredentials: passkeyCredentials(o.excluded),
            authenticatorSelection: { residentKey: "required", userVerification: "required" },
            attestation: "none",
            timeout: PASSKEY_TIMEOUT,
          },
        });
        return { credentialId: passkeyText(r.rawId), clientDataJSON: passkeyText(r.response.clientDataJSON), attestationObject: passkeyText(r.response.attestationObject) };
      });
    },
    signIn(_c, o) {
      return run(async (c) => {
        const r = await c.get({
          publicKey: { challenge: passkeyBytes(o.challenge), rpId: o.rpId, allowCredentials: passkeyCredentials(o.allowed), userVerification: "required", timeout: PASSKEY_TIMEOUT },
        });
        const p = r.response;
        return {
          credentialId: passkeyText(r.rawId),
          clientDataJSON: passkeyText(p.clientDataJSON),
          authenticatorData: passkeyText(p.authenticatorData),
          signature: passkeyText(p.signature),
          userHandle: p.userHandle && p.userHandle.byteLength ? passkeyText(p.userHandle) : null,
        };
      });
    },
  };
}
