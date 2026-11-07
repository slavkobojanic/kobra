public service NotesService {
    @Inject let api: NotesProtocol

    @State var found: List<NoteSummary> = []

    func search(title: String) async {
        do {
            found = try await api.find(title: title)
        } catch _ {
            found = []
        }
    }
}
