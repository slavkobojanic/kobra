// A case without payload is a value of the enum; implicit members resolve against the expected type.
enum Status {
    case active
    case paused
}

let s: Status = .active
              //^^^^^^^ type: corpus.Status
               //^^^^^^ resolves: member corpus.Status.active
let t = Status.paused
  //^ type: corpus.Status
