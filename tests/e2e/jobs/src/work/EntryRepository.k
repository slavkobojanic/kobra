import k.crypto.Uuid

/// The lines jobs wrote.
public repository EntryRepository for Entry {
    func add(_ text: String) async {
        await entries.insert(Entry(id: Uuid.random(), text: text))
    }

    func texts() async -> List<String> {
        let all = await entries.sortedBy { entry => entry.text }

        return all.map { entry => entry.text }
    }

    func count(prefix: String) async -> Int {
        let all = await entries.sortedBy { entry => entry.text }

        return all.filter { entry => entry.text.startsWith(prefix) }.count
    }
}
