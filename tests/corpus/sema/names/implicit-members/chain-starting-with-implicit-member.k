// `.name` at the start of a chain resolves against the chain's expected type.
let faded: Color = .white.opacity(0.5)
//                  ^^^^^ resolves: member Color.white
//                        ^^^^^^^ resolves: member Color.opacity(_:)
