import k.encoding.Json

/// Links to the `url` of the `Target` JSON in `data`, when it decodes.
public page Open {
    @Param let data: String = ""

    func decoded() -> Url? {
        let target: Target = try Json.decode(data) else {
            return nil
        }

        return target.url
    }

    view {
        main {
            if let url = decoded() {
                link(url: url) { "Continue" }
            } else {
                paragraph { "No link" }
            }
        }
    }
}
