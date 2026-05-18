module Main

open Fastoch.Elmish
open Fastoch.Elmish.HMR

open State
open View


Program.mkProgram init update view 
|> Program.withFastoch "app"
|> Program.run
