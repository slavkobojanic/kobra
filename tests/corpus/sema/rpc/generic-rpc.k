// An rpc can't be generic (K5514 at its parameters), in a protocol or in
// the service that serves it.
import k.rpc.RpcOnlyError

struct Item implements Codable {
    let name: String
}

protocol Store {
    rpc get<T>(id: Int) throws RpcOnlyError -> T? where T implements Codable
    rpc item(id: Int) throws RpcOnlyError -> Item
}

service Shelf implements Store {
    rpc get<T>(id: Int) throws RpcOnlyError -> T? where T implements Codable {
        return nil
    }

    rpc item(id: Int) throws RpcOnlyError -> Item {
        return Item(name: "pen")
    }
}
