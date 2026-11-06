// Inside a generic body, `T` is only known to implement its constraints: `==` on an unconstrained `T` is K2013.
func same<T>(a: T, b: T) -> Bool {
    return a == b
}

func equal<T>(a: T, b: T) -> Bool where T implements Equatable {
    return a == b
}
