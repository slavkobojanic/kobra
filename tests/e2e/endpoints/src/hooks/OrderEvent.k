/// A JSON body an endpoint decodes.
public struct OrderEvent implements Codable {
    let orderId: Int
    let status: String
}
