// Several generic parameters with function-typed parameters and several predicates.
public static func map<T, U>(xs: List<T>, f: (T) -> U) async throws MapError -> List<U> where T implements Codable, U implements Codable {}
