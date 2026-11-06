// A declared `init` replaces the memberwise initializer: its labels are the only ones accepted.
struct Temperature {
    let celsius: Double

    init(fahrenheit: Double) {
        celsius = (fahrenheit - 32.0) / 1.8
    }
}

let warm = Temperature(fahrenheit: 68.0)
  //^^^^ type: corpus.Temperature
let direct = Temperature(celsius: 20.0)
