module State

open Fastoch.Elmish

type Model = 
    { Counter: int}

type Action =
    | Incr
    | Decr
    | Reset

let init() =
    { Counter = 0}, Cmd.none

let update cmd model =
    match cmd with
    | Incr -> { model with Counter = model.Counter + 1}, Cmd.none
    | Decr -> { model with Counter = model.Counter - 1 |> max 0}, Cmd.none
    | Reset -> { model with Counter = 0}, Cmd.none