// A client-only service: it injects `LocalStorage`.
import k.storage.LocalStorage

public service TodoService {
    @Inject let storage: LocalStorage

    @State var items: List<String> = []
}
