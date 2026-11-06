// Rpc parameters use the same labels as functions.
protocol Transfers {
    rpc move(from source: AccountId, to target: AccountId, _ amount: Money) throws TransferError
}
