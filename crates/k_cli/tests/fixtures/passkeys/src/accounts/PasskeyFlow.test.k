import k.auth.PasskeyAssertion
import k.auth.PasskeyCreationOptions
import k.auth.PasskeyError
import k.test.TestAuthenticator

test PasskeyFlow {
    @Inject let authenticator: TestAuthenticator
    @Inject let flow: PasskeyFlow
    @Inject let auth: AuthProtocol

    /// Signs up as `name`, then out, with the sign-in options fetched.
    func signedUpThenOut(_ name: String) async {
        await flow.prepareSignUp()
        expect await flow.signUp(name: name) == "signed up"
        await flow.signOut()
        await flow.prepareSignIn()
    }

    test "the double starts supported, holding no passkey" {
        expect authenticator.isSupported
        expect authenticator.passkeys.isEmpty
        expect authenticator.prompts == 0
    }

    test "signing up makes a passkey and signs in" {
        await flow.prepareSignUp()

        expect await flow.signUp(name: "ada") == "signed up"
        expect authenticator.passkeys.count == 1
        expect authenticator.prompts == 1
        expect try await auth.whoAmI() == "ada"
    }

    test "signing in after signing out uses the passkey" {
        await signedUpThenOut("ada")
        expect try await auth.whoAmI() == ""

        expect await flow.signIn() == "signed in as ada"
        expect try await auth.whoAmI() == "ada"
    }

    test "EdDSA signs up and in" {
        authenticator.useAlgorithm(.edDSA)
        await signedUpThenOut("ed")

        expect await flow.signIn() == "signed in as ed"
    }

    test "RS256 signs up and in" {
        authenticator.useAlgorithm(.rs256)
        await signedUpThenOut("rsa")

        expect await flow.signIn() == "signed in as rsa"
    }

    test "ES256 is the default" {
        authenticator.useAlgorithm(.rs256)
        authenticator.useAlgorithm(.es256)
        await signedUpThenOut("ec")

        expect await flow.signIn() == "signed in as ec"
    }

    test "a dismissed sheet is cancelled, and the next one works" {
        await flow.prepareSignUp()
        authenticator.dismissNext()

        expect await flow.signUp(name: "ada") == "cancelled"
        expect authenticator.passkeys.isEmpty
        expect await flow.signUp(name: "ada") == "signed up"
    }

    test "a device with no passkey cancels the sign-in" {
        await signedUpThenOut("ada")
        authenticator.forgetPasskeys()

        expect await flow.signIn() == "cancelled"
    }

    test "an unsupported browser never opens a sheet" {
        authenticator.setSupported(false)
        await flow.prepareSignUp()

        expect await flow.signUp(name: "ada") == "unsupported"
        expect authenticator.prompts == 0
    }

    test "a passkey from another origin is refused" {
        await signedUpThenOut("ada")
        authenticator.setOrigin("https://notes.example.evil")

        expect await flow.signIn() == "origin mismatch"
        expect try await auth.whoAmI() == ""
    }

    test "a registration from another origin is refused" {
        authenticator.setOrigin("http://phish.example")
        await flow.prepareSignUp()

        expect await flow.signUp(name: "ada") == "origin mismatch"
    }

    test "without user verification nothing is accepted" {
        authenticator.setUserVerified(false)
        await flow.prepareSignUp()

        expect await flow.signUp(name: "ada") == "user not verified"
    }

    test "a used challenge can't be used again" {
        await signedUpThenOut("ada")
        expect flow.request != nil
        guard let options = flow.request else {
            return
        }
        let assertion = try await authenticator.signIn(options)

        expect try await auth.signIn(assertion: assertion) == "signed in as ada"
        await flow.signOut()
        expect try await auth.signIn(assertion: assertion) == "challenge mismatch"
    }

    test "a sign-in challenge doesn't register, nor a registration's sign in" {
        await signedUpThenOut("ada")
        expect flow.request != nil
        guard let request = flow.request else {
            return
        }
        let wrongCeremony = PasskeyCreationOptions(challenge: request.challenge, rpId: request.rpId, rpName: request.rpId, userHandle: "AAAA", excluded: [])
        let registration = try await authenticator.register(wrongCeremony, name: "eve")

        expect try await auth.signUp(name: "eve", registration: registration) == "challenge mismatch"
    }

    test "a newer challenge replaces the older one" {
        await signedUpThenOut("ada")
        expect flow.request != nil
        guard let older = flow.request else {
            return
        }
        await flow.prepareSignIn()
        let assertion = try await authenticator.signIn(older)

        expect try await auth.signIn(assertion: assertion) == "challenge mismatch"
    }

    test "a counter that goes back is a cloned passkey" {
        await signedUpThenOut("ada")
        authenticator.setSignCount(5)
        expect await flow.signIn() == "signed in as ada"
        await flow.signOut()
        await flow.prepareSignIn()

        authenticator.setSignCount(5)

        expect await flow.signIn() == "sign count regressed"
    }

    test "an assertion for another credential is refused" {
        await signedUpThenOut("ada")
        expect flow.request != nil
        guard let options = flow.request else {
            return
        }
        let real = try await authenticator.signIn(options)
        let swapped = PasskeyAssertion(credentialId: real.credentialId, clientDataJSON: real.clientDataJSON, authenticatorData: real.authenticatorData, signature: real.signature, userHandle: "AAAA")

        expect try await auth.signIn(assertion: swapped) == "credential mismatch"
    }

    test "a tampered signature is refused" {
        await signedUpThenOut("ada")
        expect flow.request != nil
        guard let options = flow.request else {
            return
        }
        let real = try await authenticator.signIn(options)
        let forged = PasskeyAssertion(credentialId: real.credentialId, clientDataJSON: real.clientDataJSON, authenticatorData: real.authenticatorData, signature: real.authenticatorData, userHandle: real.userHandle)

        expect try await auth.signIn(assertion: forged) == "invalid signature"
    }

    test "an unknown passkey isn't looked up further" {
        await signedUpThenOut("ada")
        expect flow.request != nil
        guard let options = flow.request else {
            return
        }
        let real = try await authenticator.signIn(options)
        let other = PasskeyAssertion(credentialId: "bm9wZQ", clientDataJSON: real.clientDataJSON, authenticatorData: real.authenticatorData, signature: real.signature, userHandle: nil)

        expect try await auth.signIn(assertion: other) == "unknown passkey"
    }

    test "an excluded passkey is already registered on the device" {
        await flow.prepareSignUp()
        expect await flow.signUp(name: "ada") == "signed up"
        expect flow.creation != nil
        guard let options = flow.creation else {
            return
        }
        let again = PasskeyCreationOptions(challenge: options.challenge, rpId: options.rpId, rpName: options.rpName, userHandle: options.userHandle, excluded: authenticator.passkeys)

        expect throws PasskeyError.alreadyRegistered {
            try await authenticator.register(again, name: "ada")
        }
    }
}
