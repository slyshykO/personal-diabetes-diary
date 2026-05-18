module Main

open Fastoch.Elmish
open Fastoch.Elmish.HMR

open State
open View


Program.mkProgram init update view
|> Program.withSubscription subscriptions
|> Program.withFastoch "app"
|> Program.run
