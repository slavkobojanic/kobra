import testing.StripeTestEvent

test StripeSignature {
    let secret = Secret(StripeTestEvent.secret)
    let signedAt = Date(unixSeconds: 1793966400)

    // Parsing

    test "a header yields its timestamp and every v1 signature" {
        let signature = StripeSignature.parse(header: "t=1793966400,v1=00ff,v1=abcd")

        expect signature?.signedAt == signedAt
        expect signature?.candidates.count == 2
    }

    test "a header as Stripe sends it parses to its digest" {
        let header = await StripeTestEvent.signature(payload: "payload", signedAt: signedAt)
        let digest = await StripeTestEvent.digest(payload: "payload", signedAt: signedAt)

        expect StripeSignature.parse(header: header)?.candidates == [digest.bytes]
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
        let digest = await StripeTestEvent.digest(payload: "payload", signedAt: signedAt)
        let signature = StripeSignature(signedAt: signedAt, candidates: [digest.bytes])

        expect await signature.matches(payload: "payload".utf8, secret: secret)
    }

    test "a payload changed by one character doesn't match" {
        let digest = await StripeTestEvent.digest(payload: "payload", signedAt: signedAt)
        let signature = StripeSignature(signedAt: signedAt, candidates: [digest.bytes])

        let matched = await signature.matches(payload: "payloaD".utf8, secret: secret)

        expect not matched
    }

    test "a signature made with another secret doesn't match" {
        let digest = await StripeTestEvent.digest(payload: "payload", secret: "whsec_other", signedAt: signedAt)
        let signature = StripeSignature(signedAt: signedAt, candidates: [digest.bytes])

        let matched = await signature.matches(payload: "payload".utf8, secret: secret)

        expect not matched
    }

    test "the timestamp is part of what is signed" {
        let digest = await StripeTestEvent.digest(payload: "payload", signedAt: signedAt)
        let forged = StripeSignature(signedAt: signedAt + 1.second, candidates: [digest.bytes])

        let matched = await forged.matches(payload: "payload".utf8, secret: secret)

        expect not matched
    }

    test "while a secret is rolled, either v1 signature may match" {
        let old = await StripeTestEvent.digest(payload: "payload", secret: "whsec_old", signedAt: signedAt)
        let current = await StripeTestEvent.digest(payload: "payload", signedAt: signedAt)
        let signature = StripeSignature(signedAt: signedAt, candidates: [old.bytes, current.bytes])

        expect await signature.matches(payload: "payload".utf8, secret: secret)
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

        expect not signature.isFresh(now: signedAt + 5.minutes + 1.second, tolerance: 5.minutes)
    }

    test "a signature too far in the future is stale" {
        let signature = StripeSignature(signedAt: signedAt, candidates: [])

        expect not signature.isFresh(now: signedAt - 5.minutes - 1.second, tolerance: 5.minutes)
    }
}
