// A protocol with typed errors, Codable payloads, a server stream and guards after the signature,
// implemented by a server-only service and called through a client-side service.
import k.auth.Authenticated
import k.auth.RateLimit
import k.rpc.RpcFailure
import k.rpc.RpcOnlyError

struct AddItemRequest implements Codable {
    let productId: Int
}

struct CartItem implements Codable {
    let productId: Int
    let quantity: Int
}

enum CartError implements Error, Codable {
    case outOfStock
    case rpcFailure(RpcFailure)
}

protocol CartProtocol requires Authenticated {
//       ^^^^^^^^^^^^ side: shared
    rpc list() throws RpcOnlyError -> List<CartItem>
    rpc add(request: AddItemRequest) throws CartError -> CartItem requires RateLimit(100.perMinute)
    rpc watch() throws CartError -> Stream<CartItem>
}

service CartApi implements CartProtocol {
//      ^^^^^^^ side: server-only
    rpc list() async throws RpcOnlyError -> List<CartItem> {
        return []
    }

    rpc add(request: AddItemRequest) async throws CartError -> CartItem {
        throw CartError.outOfStock
    }

    rpc watch() async throws CartError -> Stream<CartItem> {
        throw CartError.outOfStock
    }
}

service CartService {
//      ^^^^^^^^^^^ side: shared
    @Inject let api: CartProtocol

    @State var items: List<CartItem> = []

    func add(productId: Int) async throws CartError {
        let item = try await api.add(request: AddItemRequest(productId: productId))
        //         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ type: corpus.CartItem
        //                       ^^^ resolves: member corpus.CartProtocol.add(request:)
        items.append(item)
    }
}
