import k.crypto.EcdsaP256PrivateKey
import k.crypto.Ed25519PublicKey
import k.crypto.JwkSet
import k.crypto.Jws
import k.crypto.Jwt
import k.crypto.JwtError
import k.crypto.KeyStoreError
import k.crypto.RsaPublicKey
import k.storage.StoragePersistence
import k.test.MemoryKeyStore
import k.test.TestClock
import k.test.TestKeys

test Billing {
    use SigningConfig(signingKey: Secret(TestKeys.ed25519(1)), billingKey: TestKeys.ed25519(1).publicKey) for SigningConfig

    @Inject let billing: Billing
    @Inject let clock: TestClock

    test "a token verifies with the configured public key until it expires" {
        let token = await billing.issue(sub: "ada", plan: "pro")

        let claims = try await billing.check(token) else {
            expect false
            return
        }
        expect claims.plan == "pro"

        clock.advance(by: 2.hours)
        expect throws JwtError.expired {
            try await billing.check(token)
        }
    }

    test "the algorithm comes from the key, never from the token" {
        let token = await billing.issue(sub: "ada", plan: "pro")
        let other = await EcdsaP256PrivateKey.generate()

        expect throws JwtError.wrongAlgorithm("EdDSA") {
            let refused: BillingClaims = try await Jwt.verify(token, key: other.publicKey, issuer: "https://billing.internal", audience: "notes", now: clock.now)
        }
        expect throws JwtError.badSignature {
            try await Jws.verify(token, key: TestKeys.ed25519(2).publicKey)
        }
        expect throws JwtError.wrongAudience {
            let refused: BillingClaims = try await Jwt.verify(token, key: TestKeys.ed25519(1).publicKey, issuer: "https://billing.internal", audience: "shop", now: clock.now)
        }
        expect throws JwtError.malformed {
            try await Jws.verify("not.a.token", key: TestKeys.ed25519(1).publicKey)
        }
    }

    test "a JWK set picks the key by kid" {
        let token = await billing.issue(sub: "ada", plan: "pro")
        let jwks = "{{\"keys\": [{TestKeys.ed25519(1).publicKey.jwk.replace("\"alg\"", with: "\"kid\":\"billing-1\",\"alg\"")}, {TestKeys.ecdsaP256(1).publicKey.jwk}]}"

        guard let set = JwkSet.parse(jwks) else {
            expect false
            return
        }
        expect set.keyIds == ["billing-1"]
        let claims: BillingClaims = try await Jwt.verify(token, key: set, issuer: "https://billing.internal", audience: "notes", now: clock.now) else {
            BillingClaims(iss: "", aud: "", sub: "", plan: "")
        }
        expect claims.sub == "ada"
    }

    test "an HMAC secret signs and verifies a JWS" {
        let secret = Secret("a shared secret of at least 32 bytes!")
        let jws = await Jws.sign("payload".utf8, key: secret)

        expect try await Jws.verify(jws, key: secret) else { "".utf8 } == "payload".utf8
        expect throws JwtError.wrongAlgorithm("HS256") {
            try await Jws.verify(jws, key: TestKeys.ed25519(1).publicKey)
        }
    }
}

test Formats {
    test "public keys round-trip through JWK and PEM" {
        let key = TestKeys.ed25519(1).publicKey

        expect Ed25519PublicKey.parse(jwk: key.jwk) == key
        expect Ed25519PublicKey.parse(pem: key.pem) == key
        expect key.pem.startsWith("-----BEGIN PUBLIC KEY-----")
        expect Ed25519PublicKey.parse(jwk: TestKeys.x25519(1).publicKey.jwk) == nil
    }

    test "an RSA key from RFC 7515 verifies its example" {
        let jwk = "{{\"kty\":\"RSA\",\"e\":\"AQAB\",\"n\":\"ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ\"}"
        let token = "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw"

        guard let key = RsaPublicKey.parse(jwk: jwk) else {
            expect false
            return
        }
        expect RsaPublicKey.parse(pem: key.pem) == key
        let payload = try await Jws.verify(token, key: key) else { "".utf8 }
        expect payload.count == 70
    }
}

test KeyKeeper {
    @Inject let keeper: KeyKeeper
    @Inject let keys: MemoryKeyStore

    test "a kept key comes back, and is gone once forgotten" {
        let first = try await keeper.deviceKey() else {
            expect false
            return
        }
        let sealed = await first.seal("kept")
        expect keys.names == ["device"]

        let again = try await keeper.deviceKey() else {
            expect false
            return
        }
        expect try await again.open(sealed) else { "" } == "kept"

        try await keeper.forget() else { return }
        expect keys.names == []
    }

    test "a blocked store is unavailable, and persistence is what the test says" {
        keys.failAll(error: .unavailable)

        expect throws KeyStoreError.unavailable {
            try await keeper.deviceKey()
        }
        expect await keeper.persist() == .persistent

        keys.setPersistence(.bestEffort)
        expect await keeper.persist() == StoragePersistence.bestEffort
    }
}
