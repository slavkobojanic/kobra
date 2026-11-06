import testing.StripeTestEvent

test StripeSignature {
    let secret = StripeTestEvent.secret
    let signedAt = Date(unixSeconds: 1793966400)

    // Parsing

    test "a header yields its timestamp and every v1 signature" {
        let signature = StripeSignature.parse(header: "t=1793966400,v1=00ff,v1=abcd")

        expect signature?.signedAt == signedAt
        expect signature?.candidates.count == 2
    }

    test "other schemes, unknown entries and whitespace are ignored" {
        let signature = StripeSignature.parse(header: "t=1793966400, v0=abcd, v1=00ff, schema")

        expect signature?.candidates.count == 1
    }

    test "a v1 entry that isn't hex is skipped" {
        let signature = StripeSignature.parse(header: "t=1793966400,v1=not-hex,v1=00ff")

        expect signature?.candidates.count == 1
    }

    test "a header without a numeric timestamp is rejected" {
        expect StripeSignature.parse(header: "v1=00ff") == nil
        expect StripeSignature.parse(header: "t=yesterday,v1=00ff") == nil
        expect StripeSignature.parse(header: "") == nil
    }

    test "a header without a usable v1 signature is rejected" {
        expect StripeSignature.parse(header: "t=1793966400,v0=abcd") == nil
        expect StripeSignature.parse(header: "t=1793966400,v1=not-hex") == nil
    }

    // Matching

    test "a signature over the exact payload matches" {
        let header = StripeTestEvent.signature(payload: "payload", signedAt: signedAt)
        let signature = StripeSignature.parse(header: header)

        expect signature?.matches(payload: "payload".utf8, secret: secret) == true
    }

    test "a payload changed by one character doesn't match" {
        let header = StripeTestEvent.signature(payload: "payload", signedAt: signedAt)
        let signature = StripeSignature.parse(header: header)

        expect signature?.matches(payload: "payloaD".utf8, secret: secret) == false
    }

    test "a signature made with another secret doesn't match" {
        let header = StripeTestEvent.signature(payload: "payload", secret: "whsec_other", signedAt: signedAt)
        let signature = StripeSignature.parse(header: header)

        expect signature?.matches(payload: "payload".utf8, secret: secret) == false
    }

    test "the timestamp is part of what is signed" {
        let header = StripeTestEvent.signature(payload: "payload", signedAt: signedAt)
        let forged = header.replacing("t=1793966400", with: "t=1793966401")
        let signature = StripeSignature.parse(header: forged)

        expect signature?.matches(payload: "payload".utf8, secret: secret) == false
    }

    test "while a secret is rolled, either v1 signature may match" {
        let old = StripeTestEvent.signature(payload: "payload", secret: "whsec_old", signedAt: signedAt)
        let current = StripeTestEvent.signature(payload: "payload", signedAt: signedAt)
        let currentDigest = current.split(separator: "v1=")[1]
        let signature = StripeSignature.parse(header: "{old},v1={currentDigest}")

        expect signature?.candidates.count == 2
        expect signature?.matches(payload: "payload".utf8, secret: secret) == true
    }

    // Freshness

    test "a signature within the tolerance is fresh, in either direction" {
        let signature = StripeSignature(signedAt: signedAt, candidates: [])

        expect signature.isFresh(now: signedAt, tolerance: 5.minutes)
        expect signature.isFresh(now: signedAt + 5.minutes, tolerance: 5.minutes)
        expect signature.isFresh(now: signedAt - 5.minutes, tolerance: 5.minutes)
    }

    test "a signature older than the tolerance is stale" {
        let signature = StripeSignature(signedAt: signedAt, candidates: [])

        expect signature.isFresh(now: signedAt + 5.minutes + 1.second, tolerance: 5.minutes) == false
    }

    test "a signature too far in the future is stale" {
        let signature = StripeSignature(signedAt: signedAt, candidates: [])

        expect signature.isFresh(now: signedAt - 5.minutes - 1.second, tolerance: 5.minutes) == false
    }
}
