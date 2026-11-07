// Copying a list of class instances copies the references: both lists see a change to an element.
class Light {
    var on: Bool = false
}

func main() -> List<Bool> {
    let lights = [Light(), Light()]
    var copy = lights
    copy[0].on = true
    copy.append(Light())
    return [lights[0].on, copy[0].on, lights[1].on, lights.count == 2, copy.count == 3]
}
