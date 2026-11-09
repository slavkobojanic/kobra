import k.crypto.OpenError
import k.crypto.Sealed
import k.encoding.Json
import k.test.TestKeys

test Vault {
    test "a vault unlocks with its passphrase and opens what it sealed" {
        let created = await Vault.create(passphrase: "correct horse")
        let sealed = await created.vault.seal(PrivateText(body: "hi"), note: 1)

        let again = try await Vault.unlock(created.stored, passphrase: "correct horse") else {
            expect false
            return
        }

        expect try await again.open(sealed, note: 1) else { PrivateText(body: "") } == PrivateText(body: "hi")
    }

    test "a wrong passphrase doesn't unlock" {
        let stored = (await Vault.create(passphrase: "correct horse")).stored

        expect throws VaultError.wrongPassphrase {
            try await Vault.unlock(stored, passphrase: "battery staple")
        }
    }

    test "the server can't move a ciphertext to another note" {
        let vault = (await Vault.create(passphrase: "correct horse")).vault
        let sealed = await vault.seal(PrivateText(body: "hi"), note: 1)

        expect throws VaultError.tampered {
            try await vault.open(sealed, note: 2)
        }
    }

    test "test salts are fast, and a stored vault round-trips as JSON" {
        let stored = (await Vault.create(passphrase: "p")).stored

        expect stored.salt.iterations == 1000
        expect try Json.decode(Json.encode(stored)) else { stored } == stored
    }

    test "randomness repeats in every run of a test" {
        let sealed: Sealed<Bytes> = await TestKeys.aesGcm(1).seal("x".utf8)

        let text = "{sealed}"

        expect text == "k1.AS3M8X8b4R8xk8mQO2OBWvEk4tnuxMLzuHDGB---"
    }

    test "an authentic value of another type is undecodable" {
        let key = TestKeys.aesGcm(2)
        let sealed: Sealed<String> = await key.seal("text")
        let reread: Sealed<Int>? = Sealed.parse("{sealed}")
        var undecodable = false
        if let reread {
            do {
                let n = try await key.open(reread)
                expect n == 0
            } catch .undecodable {
                undecodable = true
            } catch .authenticationFailed {
                undecodable = false
            }
        }

        expect undecodable
        expect throws OpenError.authenticationFailed {
            try await TestKeys.aesGcm(3).open(sealed)
        }
    }
}
