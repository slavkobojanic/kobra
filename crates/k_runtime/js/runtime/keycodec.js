/**
 * @module k:runtime/keycodec
 *
 * The `Codable` codec of `k.crypto.EcdsaP256PublicKey` on its own
 * (docs/design/50-push.md, "As built"), for a client that receives a
 * push server key by rpc and does nothing else with `k.crypto`: the key is
 * held as its text, so decoding only checks that text, and the whole
 * crypto chunk (about 5.6 KB gzip) stays unloaded. `crypto.js` exports the
 * same codec; the bundler takes this chunk's (codegen.md §16).
 *
 * The text is unpadded, canonical base64url of the 65-byte uncompressed
 * point: 87 characters, the first byte `0x04` (`B` then `A`–`P`), and the
 * last character's two spare bits zero.
 */

import { DecodeFailure } from "./std.js";

/** The text form of an uncompressed P-256 point. */
const POINT = new RegExp("^B[A-P][A-Za-z0-9_-]{84}[AEIMQUYcgkosw048]$");

/** The codec of `EcdsaP256PublicKey`, which accepts only its text form. */
export const EcdsaP256PublicKey$codec = {
  key: true,
  encode: (v) => v,
  decode: (j, p) => {
    if (typeof j === "string" && POINT.test(j)) return j;
    throw new DecodeFailure(p || "$", "expected a EcdsaP256PublicKey");
  },
};
