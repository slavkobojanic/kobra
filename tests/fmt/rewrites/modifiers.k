public struct Profile {
    public let name: String
    public   static func empty() -> Profile { Profile(name: "") }
    public init(name: String) {
        self.name = name
    }
    private let secret: String = ""
}

public service ProfileService {
    @Inject public let clock: Clock
    @Inject private let storage: Storage
    @Inject let api: ProfileApi
}

public enum Tone {
    public case neutral
}
