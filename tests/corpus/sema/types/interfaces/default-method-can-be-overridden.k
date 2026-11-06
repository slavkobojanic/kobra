// A conforming type may provide its own version of a default method.
interface Greeter {
    func greeting() -> String {
        return "Hello"
    }
}

struct Robot implements Greeter {
    func greeting() -> String {
        return "Beep"
    }
}

let text = Robot().greeting()
                 //^^^^^^^^ resolves: member corpus.Robot.greeting()
