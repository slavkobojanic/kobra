// A value is accepted where a supertype is expected inside the read-only
// positions of `List`, `Set`, `Map` values, optionals, `Stream`, `Result`
// and function results: a class is the interface it implements, a value
// its optional, a component a `View`. A list's struct elements convert to
// their interface too (each gets its adapter).
interface Animal {
    func name() -> String
}

class Dog implements Animal {
    func name() -> String {
        return "dog"
    }
}

interface Priced {
    func price() -> Int
}

struct Card implements Priced {
    let cents: Int

    func price() -> Int {
        return cents
    }
}

struct Shelter {
    let animals: List<Animal>
}

func names(_ animals: List<Animal>) -> Int {
    return animals.count
}

func keep(
    dogs: List<Dog>,
    byName: Map<String, Dog>,
    found: Result<Dog, Never>,
    stream: Stream<Dog>,
    adopt: () -> Dog,
    cards: List<Card>,
) -> Int {
    let animals: List<Animal> = dogs
    let maybe: List<Dog?> = dogs
    let named: Map<String, Animal> = byName
    let wide: Result<Animal, Never> = found
    let all: Stream<Animal> = stream
    let pick: () -> Animal = adopt
    let priced: List<Priced> = cards
    let somePriced: List<Priced>? = cards
    let shelter = Shelter(animals: dogs)
    return names(dogs) + shelter.animals.count
}
