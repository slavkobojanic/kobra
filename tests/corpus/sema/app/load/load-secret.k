// `Secret<T>` isn't `Codable`, so it can't be loaded.
func token() async -> Secret<String> {
    return Secret("x")
}

page Token {
    @Load let token: Secret<String> = await token()

    view {
        text { "token" }
    }

    loading {
    }
}
