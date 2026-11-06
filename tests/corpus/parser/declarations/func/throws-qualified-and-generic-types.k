// `throws` takes any type.
func a() throws errors.Checkout {}
func b() throws Wrapped<NetworkError> {}
