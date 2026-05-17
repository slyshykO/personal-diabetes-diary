open Fable.Core

open Fable
open Fable.Import

open Browser.Types

open Fastoch
open Fastoch.Feliz
open Fastoch.Elmish
open Fastoch.Elmish.HMR

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

let view dispatch =

    fun model  ->
    Html.div [
        Html.ul [
            Html.li [
                prop.classes [ "text-lg"; "font-bold" ]
                prop.text   $"{model.Counter}"
                if model.Counter = 0 then
                    prop.style [ style.color "green"]
                elif model.Counter >= 10 then
                    prop.style [ style.color "red"; style.backgroundColor "lightblue"]
                
                prop.onWheel (Hooks.callback((), fun e ->
                    (if e.deltaY > 0 then Incr else Decr) |> dispatch) 
                )
            ]
        ]
        Html.button [
            prop.classes [ "btn"; "btn-primary" ]
            prop.text "+"
            prop.onClick (Hooks.callback((), fun _ -> dispatch Incr ))
        ]
        Html.button [
            prop.classes [ "btn"; "btn-primary" ]
            prop.text "-"
            prop.onClick (Hooks.callback((), fun _ -> dispatch Decr ))
        ]
        Html.button [
            prop.classes [ "btn"; "btn-secondary" ]
            prop.text "Reset"
            prop.onClick (Hooks.callback((), fun _ -> dispatch Reset ))
        ]
    ]

Program.mkProgram init update view 
|> Program.withFastoch "app"
|> Program.run
