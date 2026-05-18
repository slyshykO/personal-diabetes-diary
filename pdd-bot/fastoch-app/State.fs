module State

open Fastoch.Elmish

type Route =
    | Home
    | Counter
    | NotFound of string list

let routeFromPathParts parts =
    match parts with
    | [] -> Home
    | [ "counter" ] -> Counter
    | _ -> NotFound parts

let pathFromRoute route =
    match route with
    | Home -> "/"
    | Counter -> "/counter"
    | NotFound [] -> "/"
    | NotFound parts -> Router.formatParts parts

type Model =
    { Counter: int
      Route: Route }

type Action =
    | Incr
    | Decr
    | Reset
    | Navigate of Route
    | UrlChanged of string list

let init() =
    { Counter = 0
      Route = Router.current () |> routeFromPathParts }, Cmd.none

let update cmd model =
    match cmd with
    | Incr -> { model with Counter = model.Counter + 1}, Cmd.none
    | Decr -> { model with Counter = model.Counter - 1 |> max 0}, Cmd.none
    | Reset -> { model with Counter = 0}, Cmd.none
    | Navigate route ->
        model, route |> pathFromRoute |> Router.navigate UrlChanged
    | UrlChanged parts ->
        printf "URL changed: %A" parts
        { model with Route = routeFromPathParts parts }, Cmd.none

let subscriptions _ =
    Router.subscribe UrlChanged
