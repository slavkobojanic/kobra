import k.encoding.Json

/// What has been picked, kept as JSON text the way it would be stored;
/// its generic members take their type arguments written out or inferred.
public service Basket {
    @State var saved: List<String> = []

    func keep<T>(_ value: T) where T implements Codable {
        saved = saved + [Json.encode(value)]
    }

    func last<T>() -> T? where T implements Codable {
        guard let text = saved.last else { return nil }
        let value: T? = try Json.decode<T>(text) else { nil }
        return value
    }
}
