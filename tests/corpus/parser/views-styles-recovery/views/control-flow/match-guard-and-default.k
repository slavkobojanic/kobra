// Match arms with a `where` guard and a `default` arm.
component T {
    view {
        match status {
            case .shipped(let tracking) where tracking.isDelayed {
                Badge(text: "Late")
            }

            default {
                "On time"
            }
        }
    }
}
