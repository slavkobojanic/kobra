// A repository can't inject a protocol.
import k.rpc.RpcOnlyError

protocol CartProtocol {
    rpc items() throws RpcOnlyError -> List<Int>
}

entity Note {
    @Id let id: Int
}

repository NoteRepository for Note {
    @Inject let api: CartProtocol
}
