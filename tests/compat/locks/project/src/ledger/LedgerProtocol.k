/// The ledger's rpcs (`rpc.lock`).
public protocol LedgerProtocol {
    rpc balance(account: String) throws BalanceError -> Balance
}
