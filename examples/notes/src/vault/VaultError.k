import k.rpc.RpcFailure

/// Why a vault operation failed.
public enum VaultError implements Error {
    /// The account hasn't set up a vault yet.
    case noVault

    /// The account has a vault already, so another can't replace it.
    case alreadyCreated

    /// The passphrase doesn't unwrap the vault key.
    case wrongPassphrase

    /// This browser holds no vault key: unlock the vault first.
    case locked

    /// The server returned a ciphertext that isn't this note's.
    case tampered

    /// There is no note with this id, or it belongs to another account.
    case notFound

    /// The call failed outside `VaultApi`: nobody is signed in, or the
    /// browser couldn't reach the server.
    case rpcFailure(RpcFailure)
}
