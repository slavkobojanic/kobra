/**
 * @module k:/tests/passkeys.test.js
 *
 * `k.auth.BrowserPasskeys` (docs/design/51-passkeys.md; docs/compiler/runtime.md §12.17) against
 * a scripted `navigator.credentials` and `PublicKeyCredential`, written from WebAuthn Level 3: no
 * WebAuthn outside a secure context or without `PublicKeyCredential`; `create` with the server's
 * challenge, RP, user handle, the three algorithms, a required resident key, required user
 * verification, attestation `"none"` and the excluded credentials as bytes; `get` with the
 * challenge, RP id and allowed credentials; the `ArrayBuffer`s of the result as base64url; and
 * `NotAllowedError`, `AbortError`, `InvalidStateError` and `SecurityError` as `PasskeyError`s.
 */
import { assert, assertDeepEq, assertEq, test } from "k:testing";
import { BrowserPasskeys } from "k:/runtime/passkeys.js";
import { unitCase } from "k:runtime";

/** `bytes` as an `ArrayBuffer`. */
const buffer = (bytes) => Uint8Array.from(bytes).buffer;

/** The server's options, base64url as `PasskeyVerifier` writes them. */
const CREATION = { challenge: "AAECAw", rpId: "notes.example", rpName: "notes.example", userHandle: "BwgJ", excluded: ["CgsM"] };
const REQUEST = { challenge: "AAECAw", rpId: "notes.example", allowed: [] };

/** A scripted browser: `calls` records each ceremony's options, `fail` names the next rejection. */
function fakeCredentials() {
  const fake = { calls: [], fail: null, userHandle: [7, 8, 9] };
  const reject = () => {
    if (!fake.fail) return;
    const name = fake.fail;
    fake.fail = null;
    throw Object.assign(new Error(name), { name });
  };
  fake.credentials = {
    create: async (options) => {
      fake.calls.push(options);
      reject();
      return { rawId: buffer([1, 2, 3]), response: { clientDataJSON: buffer([123, 125]), attestationObject: buffer([0xa0]) } };
    },
    get: async (options) => {
      fake.calls.push(options);
      reject();
      return {
        rawId: buffer([1, 2, 3]),
        response: { clientDataJSON: buffer([123, 125]), authenticatorData: buffer([4, 5]), signature: buffer([6]), userHandle: fake.userHandle ? buffer(fake.userHandle) : null },
      };
    },
  };
  return fake;
}

/** Runs `f` in a browser with `fake`'s credentials, secure or not, with or without WebAuthn. */
async function inBrowser(fake, secure, f, { withWebAuthn = true } = {}) {
  const g = globalThis;
  const saved = [g.navigator, g.isSecureContext, g.PublicKeyCredential];
  g.navigator = { credentials: fake.credentials };
  g.isSecureContext = secure;
  g.PublicKeyCredential = withWebAuthn ? function PublicKeyCredential() {} : undefined;
  try {
    return await f();
  } finally {
    [g.navigator, g.isSecureContext, g.PublicKeyCredential] = saved;
  }
}

/** Settles `p` into `{ok}` or `{error}`. */
const settle = (p) => p.then((ok) => ({ ok }), (error) => ({ error }));

test("outside a secure context, or without WebAuthn, passkeys are unsupported and nothing opens", async () => {
  for (const [secure, withWebAuthn] of [[false, true], [true, false]]) {
    const fake = fakeCredentials();
    await inBrowser(
      fake,
      secure,
      async () => {
        const p = BrowserPasskeys();
        assert(!p.isSupported);
        assertDeepEq(await settle(p.register(null, CREATION, "ada")), { error: unitCase(0) });
        assertDeepEq(await settle(p.signIn(null, REQUEST)), { error: unitCase(0) });
        assertEq(fake.calls.length, 0);
      },
      { withWebAuthn },
    );
  }
});

test("register asks for a discoverable, user-verified credential with attestation none", async () => {
  const fake = fakeCredentials();
  await inBrowser(fake, true, async () => {
    const p = BrowserPasskeys();
    assert(p.isSupported);
    const r = await p.register(null, CREATION, "ada@example.com");
    assertDeepEq(r, { credentialId: "AQID", clientDataJSON: "e30", attestationObject: "oA" });
    const o = fake.calls[0].publicKey;
    assertDeepEq([...o.challenge], [0, 1, 2, 3]);
    assertDeepEq(o.rp, { id: "notes.example", name: "notes.example" });
    assertDeepEq([...o.user.id], [7, 8, 9]);
    assertEq(o.user.name, "ada@example.com");
    assertEq(o.user.displayName, "ada@example.com");
    assertDeepEq(
      o.pubKeyCredParams.map((c) => c.alg),
      [-8, -7, -257],
    );
    assertDeepEq(
      o.excludeCredentials.map((c) => [c.type, [...c.id]]),
      [["public-key", [10, 11, 12]]],
    );
    assertDeepEq(o.authenticatorSelection, { residentKey: "required", userVerification: "required" });
    assertEq(o.attestation, "none");
    assertEq(o.timeout, 300000);
  });
});

test("signIn sends the challenge, RP id and allowed credentials, and returns the assertion's bytes", async () => {
  const fake = fakeCredentials();
  await inBrowser(fake, true, async () => {
    const p = BrowserPasskeys();
    const a = await p.signIn(null, { ...REQUEST, allowed: ["AQID"] });
    assertDeepEq(a, { credentialId: "AQID", clientDataJSON: "e30", authenticatorData: "BAU", signature: "Bg", userHandle: "BwgJ" });
    const o = fake.calls[0].publicKey;
    assertEq(o.rpId, "notes.example");
    assertEq(o.userVerification, "required");
    assertDeepEq(
      o.allowCredentials.map((c) => [...c.id]),
      [[1, 2, 3]],
    );
    fake.userHandle = null;
    assertEq((await p.signIn(null, REQUEST)).userHandle, null);
  });
});

test("rejections become PasskeyError cases", async () => {
  const fake = fakeCredentials();
  await inBrowser(fake, true, async () => {
    const p = BrowserPasskeys();
    for (const [name, error] of [
      ["NotAllowedError", 1],
      ["AbortError", 1],
      ["InvalidStateError", 2],
      ["SecurityError", 3],
      ["UnknownError", 3],
    ]) {
      fake.fail = name;
      assertDeepEq(await settle(p.register(null, CREATION, "ada")), { error: unitCase(error) }, name);
    }
    fake.fail = "NotAllowedError";
    assertDeepEq(await settle(p.signIn(null, REQUEST)), { error: unitCase(1) });
  });
});
