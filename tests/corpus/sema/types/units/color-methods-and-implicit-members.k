// Colors have methods and static members; implicit members resolve against `Color`.
let soft = Color.white.opacity(0.8)
  //^^^^ type: Color
let base: Color = .rgb(59, 130, 246)
                 //^^^ resolves: member Color.rgb(_:_:_:)
let mixed = #3B82F6.mix(.white, 20%)
  //^^^^^ type: Color
let fromHex: Color = .hex("#3B82F6")
  //^^^^^^^ type: Color
