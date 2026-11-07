// `extend Double` adds members to every Double.
extend Double {
    var half: Double { self / 2.0 }

    func clamped(to limit: Double) -> Double {
        return if self > limit { limit } else { self }
    }
}

func main() -> List<Double> {
    return [3.0.half, 9.5.clamped(to: 5.0), 1.5.clamped(to: 5.0)]
}
