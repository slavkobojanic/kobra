// `async`, `throws E` and `-> T` in their fixed order, each optional.
func a() async {}
func b() throws NetworkError {}
func c() -> Int {}
func d() async throws NetworkError {}
func e() async -> Int {}
func f() throws NetworkError -> Int {}
func g() async throws NetworkError -> Int {}
