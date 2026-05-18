module View

open Fastoch
open Fastoch.Feliz

open State

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