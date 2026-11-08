import k.http.HttpResponse
import k.http.IncomingRequest

// No metadata at all, under the app's default layout: K3027, and the fix
// adds a `metadata` block.
page Home at "/" {
    view {
        text { "Home" }
    }
}

// A metadata block without a title: K3027, and the fix adds the title to it.
page OrderHistory {
    metadata {
        description("Your orders")
    }

    view {
        text { "Orders" }
    }
}

// Its own title: nothing.
page About {
    metadata {
        title("About us")
    }

    view {
        text { "About" }
    }
}

// The title comes from its layout: nothing.
@Layout(Shop)
page Catalog {
    view {
        text { "Catalog" }
    }
}

// Out of every layout, with no title: K3027.
@NoLayout
page Bare {
    view {
        text { "Bare" }
    }
}

// A title only inside `failed` is the failure's, not the page's: K3027.
page Product {
    @Load let name: String = try await fetchName()

    view {
        text { name }
    }

    loading {
        text { "…" }
    }

    failed(let error) {
        metadata {
            title("Not found")
        }

        text { "Missing" }
    }
}

// An endpoint page has no metadata: nothing.
page Ping at "/ping" {
    @Get
    func run(request: IncomingRequest) async -> HttpResponse {
        return HttpResponse(status: .ok)
    }
}

func fetchName() async throws LoadError -> String {
    return "x"
}

enum LoadError implements Error, Codable {
    case missing
}
