// Return types may be optional, generic, tuple, function or qualified types.
func a() -> User? {}
func b() -> List<User> {}
func c() -> (Int, String) {}
func d() -> (Int) -> Bool {}
func e() -> ((Int) -> Bool)? {}
func f() -> shop.Money {}
func g() -> () async -> Void {}
