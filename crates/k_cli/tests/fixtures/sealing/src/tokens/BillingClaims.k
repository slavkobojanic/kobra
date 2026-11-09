/// The claims of a token from the billing service, a system outside K.
public struct BillingClaims implements Codable, Equatable {
    let iss: String
    let aud: String
    let sub: String
    let plan: String
}
