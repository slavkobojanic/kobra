// A case that doesn't exist, written with the enum's name, is K1203 (not K2006, which is for implicit members).
enum Status {
    case active
}

let s = Status.archived
