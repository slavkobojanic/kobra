// Parameters may have function types, including async and throwing ones.
func each(action: (Int) -> Void, load: () async throws LoadError -> Data, pick: ((Int, Int) -> Int)?) {}
