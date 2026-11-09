/// Why the vault refused.
public enum VaultError implements Error, Equatable {
    case wrongPassphrase
    case tampered
}
