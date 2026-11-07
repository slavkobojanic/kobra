import k.rpc.RpcOnlyError

/// Returns every argument unchanged.
public service EchoApi implements EchoProtocol {
    rpc record(value: Record) async throws RpcOnlyError -> Record {
        return value
    }

    rpc shapes(value: List<Shape>) async throws RpcOnlyError -> List<Shape> {
        return value
    }

    rpc maybe(value: Leaf?) async throws RpcOnlyError -> Leaf? {
        return value
    }

    rpc counts(value: Map<String, Int>) async throws RpcOnlyError -> Map<String, Int> {
        return value
    }

    rpc text(value: String) async throws RpcOnlyError -> String {
        return value
    }

    rpc number(value: Int) async throws RpcOnlyError -> Int {
        return value
    }

    rpc pair(left: Double, right: Mood) async throws RpcOnlyError -> Mood {
        return right
    }

    rpc ping() async throws RpcOnlyError {
    }
}
