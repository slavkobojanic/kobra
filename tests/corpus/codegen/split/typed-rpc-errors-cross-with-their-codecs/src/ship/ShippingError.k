import k.rpc.RpcFailure

/// A Codable error: its payload crosses the wire in its codec's form.
public enum ShippingError implements Error, Codable {
    case noSlot(reason: SlotReason, until: Date)
    case closed
    case rpcFailure(RpcFailure)
}
