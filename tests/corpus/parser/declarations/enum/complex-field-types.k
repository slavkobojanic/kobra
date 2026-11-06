// Case fields may have optional, function, tuple and generic types.
enum Handler {
    case maybe(Int?)
    case callback((Int) -> Void)
    case pair((Int, String))
    case asyncWork(() async throws NetworkError -> Data)
    case many(List<Map<String, Int>>)
}
