import k.storage.LocalStorage

public service CartService {
    @Inject let storage: LocalStorage

    @State var items: List<String> = []
}
