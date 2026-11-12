public struct Profile {
    let name: String
    static func empty() -> Profile { Profile(name: "") }
    init(name: String) {
        self.name = name
    }
    private let secret: String = ""
}

public service ProfileService {
    @Inject let clock: Clock
    @Inject let storage: Storage
    @Inject let api: ProfileApi
}

public enum Tone {
    case neutral
}
