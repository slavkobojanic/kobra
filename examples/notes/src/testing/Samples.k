// Accounts shared by the tests. They are static members added by a public
// extension in this test-only module, so every test file can write
// `AccountId.ada`, and no production code can.

import accounts.AccountId
import k.crypto.Uuid

public extend AccountId {
    /// Ada, the account most tests act as. Tests sign in with
    /// `sessions.signIn(userId: AccountId.ada)` without creating the account.
    static let ada: AccountId = AccountId(value: Uuid("6f1c2a4e-3b7d-4c59-9e21-0a8d5f3b7c10"))

    /// Ben, a second account, for checking that one account never sees
    /// another's notes.
    static let ben: AccountId = AccountId(value: Uuid("b2e4d6f8-1a3c-4e5b-8d7f-9c0b1a2d3e4f"))
}
