import accounts.AccountId
import k.auth.SessionManager
import k.crypto.Uuid
import k.test.MemoryKeyStore
import notes.NoteDraft
import notes.NotesService
import notes.PrivateText

// Integration tests, as for notes: VaultService, VaultProtocol in-process,
// VaultApi and a rolled-back database, with the test's `MemoryKeyStore`
// standing in for the browser's IndexedDB.
test VaultService {
    @Inject let vault: VaultService
    @Inject let keys: MemoryKeyStore
    @Inject let sessions: SessionManager
    @Inject let notes: NotesService

    test "a wrong passphrase doesn't unlock" {
        await sessions.signIn(userId: AccountId.ada)
        try await vault.create(passphrase: "correct horse")
        await vault.lock()

        expect throws VaultError.wrongPassphrase {
            try await vault.unlock(passphrase: "battery staple")
        }
        expect keys.names == []
    }

    test "the server can't move a ciphertext to another note" {
        await sessions.signIn(userId: AccountId.ada)
        try await vault.create(passphrase: "correct horse")
        let sealed = try await vault.seal(PrivateText(body: "hi"), note: Uuid("00000000-0000-4000-8000-000000000001"))

        expect throws VaultError.tampered {
            try await vault.open(sealed, note: Uuid("00000000-0000-4000-8000-000000000002"))
        }
    }

    test "the passphrase unlocks the vault again after locking, and the key is kept" {
        await sessions.signIn(userId: AccountId.ada)
        try await vault.create(passphrase: "correct horse")
        expect keys.names == ["notes-vault"]
        await vault.lock()
        expect keys.names == []

        try await vault.unlock(passphrase: "correct horse")

        expect keys.names == ["notes-vault"]
        await vault.restore()
        expect match vault.state {
            case .unlocked { true }
            case .locked, .unknown { false }
        }
    }

    test "a note's private text is stored sealed and opens only in the vault" {
        await sessions.signIn(userId: AccountId.ada)
        try await vault.create(passphrase: "correct horse")
        let note = try await notes.create(draft: NoteDraft(title: "Diary", body: "", tags: ""))

        try await vault.write(PrivateText(body: "dear diary"), note: note.id)

        expect try await vault.read(note: note.id) == PrivateText(body: "dear diary")
        await vault.lock()
        expect throws VaultError.locked {
            try await vault.read(note: note.id)
        }
    }

    test "another account can neither read nor replace a note's private text" {
        await sessions.signIn(userId: AccountId.ada)
        try await vault.create(passphrase: "correct horse")
        let note = try await notes.create(draft: NoteDraft(title: "Diary", body: "", tags: ""))
        try await vault.write(PrivateText(body: "dear diary"), note: note.id)

        await sessions.signIn(userId: AccountId.ben)

        expect throws VaultError.notFound {
            try await vault.read(note: note.id)
        }
        expect throws VaultError.noVault {
            try await vault.unlock(passphrase: "correct horse")
        }
    }

    test "an account has one vault" {
        await sessions.signIn(userId: AccountId.ada)
        try await vault.create(passphrase: "correct horse")

        expect throws VaultError.alreadyCreated {
            try await vault.create(passphrase: "another")
        }
    }
}
