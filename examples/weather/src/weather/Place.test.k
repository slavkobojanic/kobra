test Place {
    test "the display name joins name, region and country" {
        expect berlin(region: "Land Berlin", country: "Germany").displayName == "Berlin, Land Berlin, Germany"
    }

    test "a region that repeats the name is left out" {
        expect berlin(region: "Berlin", country: "Germany").displayName == "Berlin, Germany"
    }

    test "missing parts are skipped" {
        expect berlin(region: nil, country: "Germany").displayName == "Berlin, Germany"
        expect berlin(region: "Land Berlin", country: nil).displayName == "Berlin, Land Berlin"
        expect berlin(region: nil, country: nil).displayName == "Berlin"
    }
}

func berlin(region: String?, country: String?) -> Place {
    return Place(
        id: 2950159,
        name: "Berlin",
        region: region,
        country: country,
        latitude: 52.52437,
        longitude: 13.41053
    )
}
