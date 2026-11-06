// A missing argument of a declared `init` is K2003 at the gap before `)`.
struct Temperature {
    let celsius: Double

    init(fahrenheit: Double) {
        celsius = (fahrenheit - 32.0) / 1.8
    }
}

let cold = Temperature()
