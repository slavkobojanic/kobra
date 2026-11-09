import k.crypto.AesGcmKey
import k.crypto.EcdsaP256PrivateKey
import k.crypto.Envelope
import k.crypto.OpenError
import k.encoding.Json
import k.test.TestKeys

test Manifest {
    test "a configured key signs, and only its public key verifies" {
        let manifest = await Manifest.signed("export", with: Secret(TestKeys.ed25519(1)))

        expect await manifest.verify(trusting: TestKeys.ed25519(1).publicKey)
        expect not (await manifest.verify(trusting: TestKeys.ed25519(2).publicKey))

        let changed = Manifest(body: "other", signature: manifest.signature, signer: manifest.signer)
        expect not (await changed.verify(trusting: manifest.signer))
    }

    test "a signed manifest round-trips as JSON" {
        let manifest = await Manifest.signed("export", with: Secret(TestKeys.ed25519(1)))

        expect try Json.decode(Json.encode(manifest)) else { manifest } == manifest
        expect "{manifest.signer}".count == 43
    }

    test "an ECDSA signature verifies with its key" {
        let key = await EcdsaP256PrivateKey.generate()
        let signature = await key.sign("m".utf8)

        expect signature.bytes.count == 64
        expect await key.publicKey.verify(signature, of: "m".utf8)
        expect (await key.publicKey.fingerprint()).count == 64
    }

    test "an envelope opens with the recipient's key and context only" {
        let recipient = TestKeys.x25519(1)
        let envelope: Envelope<String> = await recipient.publicKey.seal("for you", context: "share:1".utf8)

        expect try await recipient.open(envelope, context: "share:1".utf8) else { "" } == "for you"
        expect throws OpenError.authenticationFailed {
            try await TestKeys.x25519(2).open(envelope, context: "share:1".utf8)
        }
    }

    test "a key shared through an envelope seals for the recipient" {
        let recipient = TestKeys.x25519(1)
        let noteKey = await AesGcmKey.generateExportable()
        let shared = await recipient.publicKey.wrap(noteKey)
        let sealed = await noteKey.key.seal("note")

        let theirs = try await Secret(recipient).unwrap(shared) else {
            expect false
            return
        }
        expect try await theirs.open(sealed) else { "" } == "note"
    }

    test "two agreeing keys derive the same key" {
        let alice = TestKeys.x25519(1)
        let bob = TestKeys.x25519(2)
        let a = await AesGcmKey.derive(from: await alice.agree(with: bob.publicKey), salt: "s".utf8, info: "chat/v1")
        let b = await AesGcmKey.derive(from: await bob.agree(with: alice.publicKey), salt: "s".utf8, info: "chat/v1")

        expect try await b.open(await a.seal("hi")) else { "" } == "hi"
    }
}
