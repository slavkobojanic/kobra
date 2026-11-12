import k.crypto.Uuid
import notes.PrivateText

/// Sets up, unlocks and locks the vault, and edits a note's private text
/// with it.
///
/// The vault key lives only in this browser, so this component is
/// `@Rendering(.client)`: the server renders its `loading` block, and the
/// browser swaps in the form once `VaultService` has restored the key it
/// kept, if any. The vault has its own passphrase, never the sign-in
/// password: the server receives that one at sign-in, so it must not unlock
/// anything.
@Rendering(.client)
public component PrivateTextEditor {
    /// The note whose private text this is.
    @Prop let note: Uuid

    @Inject let vault: VaultService

    /// What the person types: the passphrase, then the private text.
    @State var passphrase: String = ""
    @State var text: String = ""

    /// Whether the account has a vault yet; `nil` until known.
    @State var hasVault: Bool? = nil

    /// Whether the last save succeeded, for the status message.
    @State var isSaved: Bool = false

    /// Why the last action failed, for the alert.
    @State var problem: String? = nil

    /// Restores the key this browser kept, then opens the note's text.
    @Effect
    func start() async {
        await vault.restore()
        await load()
    }

    /// Opens the private text when the vault is unlocked; else asks whether
    /// there is a vault to unlock.
    func load() async {
        problem = nil
        do {
            match vault.state {
                case .unlocked {
                    text = (try await vault.read(note: note)).body
                }
                case .locked, .unknown {
                    hasVault = try await vault.exists()
                }
            }
        } catch let error {
            problem = message(error)
        }
    }

    /// Sets up the vault, or unlocks it, with the passphrase.
    func unlock() async {
        problem = nil
        do {
            if hasVault == false {
                try await vault.create(passphrase: passphrase)
            } else {
                try await vault.unlock(passphrase: passphrase)
            }
            passphrase = ""
            await load()
        } catch let error {
            problem = message(error)
        }
    }

    /// Seals the text in this browser and stores the ciphertext.
    func save() async {
        isSaved = false
        problem = nil
        do {
            try await vault.write(PrivateText(body: text), note: note)
            isSaved = true
        } catch let error {
            problem = message(error)
        }
    }

    /// Forgets the vault key in this browser.
    func lock() async {
        await vault.lock()
        text = ""
        isSaved = false
        await load()
    }

    /// What to tell the person about `error`.
    func message(_ error: VaultError) -> String {
        return match error {
            case .wrongPassphrase { "That passphrase doesn't open the vault." }
            case .tampered { "The stored text isn't this note's, so it wasn't opened." }
            case .locked { "The vault is locked." }
            case .noVault, .alreadyCreated { "Reload the page: the vault changed in another tab." }
            case .notFound { "This note doesn't exist, or it was deleted." }
            case .rpcFailure { "Couldn't reach K Notes. Try again." }
        }
    }

    view {
        match vault.state {
            case .unknown {
                paragraph { "Opening the vault…" }
                    .role(.status)
            }
            case .locked {
                paragraph {
                    if hasVault == false {
                        "Set up a vault to keep text that only your browsers can read. Use a passphrase that isn't your password: if you forget it, the text can't be recovered."
                    } else {
                        "The vault is locked in this browser. Its passphrase unlocks it."
                    }
                }

                form {
                    passwordField(value: bind passphrase)
                        .label("Vault passphrase")

                    button { if hasVault == false { "Set up the vault" } else { "Unlock" } }
                        .type(.submit)
                }
                .class(.editor)
                .on(submit: unlock)
            }
            case .unlocked {
                form {
                    textArea(value: bind text)
                        .label("Private text")
                        .rows(12)

                    button { "Save" }
                        .type(.submit)
                }
                .class(.editor)
                .on(submit: save)

                button { "Lock the vault" }
                    .on(click: lock)
            }
        }

        if isSaved {
            paragraph { "Saved, sealed in this browser." }
                .class(.status)
                .role(.status)
        }

        if let problem {
            paragraph { problem }
                .class(.problem)
                .role(.alert)
        }
    }

    loading {
        paragraph { "Opening the vault…" }
            .role(.status)
    }

    style {
        .editor { display: grid; gap: .md; marginBottom: .md; }
        .status { color: .muted; }
        .problem { color: .danger; }
    }
}
