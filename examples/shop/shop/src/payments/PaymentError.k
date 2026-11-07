/// Why a `PaymentGateway` couldn't start a checkout.
public enum PaymentError implements Error {
    /// The provider couldn't be reached, timed out, or failed on its side.
    /// Trying again later may work.
    case unavailable

    /// The provider understood the request and refused it, for example
    /// because the API key is wrong. Trying again won't help.
    case rejected(reason: String)
}
