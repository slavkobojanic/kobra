// A qualified enum case that doesn't exist is K1203 (an explicit member access, not an implicit member).
enum Light {
    case red
    case green
}

func stop() -> Light {
    return Light.purple
}
