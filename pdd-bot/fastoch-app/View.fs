module View

open Browser.Types
open Fastoch
open Fastoch.Feliz

open State

let private isPlainLeftClick (e: MouseEvent) =
    e.button = 0.0 && not (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey)

let private navLink dispatch currentRoute route (label: string) =
    let isActive = currentRoute = route

    Html.a [
        prop.href (pathFromRoute route)
        prop.classes [
            "btn"
            if isActive then "btn-primary" else "btn-ghost"
        ]
        prop.text label
        prop.onClick (Hooks.callback((), fun e ->
            if isPlainLeftClick e then
                e.preventDefault()
                dispatch (Navigate route)))
    ]

let private counterView dispatch model =
    Html.div [
        prop.classes [ "space-y-4" ]
        prop.children [
            Html.ul [
                Html.li [
                    prop.classes [ "text-lg"; "font-bold" ]
                    prop.text $"{model.Counter}"
                    if model.Counter = 0 then
                        prop.style [ style.color "green"]
                    elif model.Counter >= 10 then
                        prop.style [ style.color "red"; style.backgroundColor "lightblue"]

                    prop.onWheel (Hooks.callback((), fun e ->
                        (if e.deltaY > 0 then Incr else Decr) |> dispatch)
                    )
                ]
            ]
            Html.div [
                prop.classes [ "join" ]
                prop.children [
                    Html.button [
                        prop.classes [ "btn"; "btn-primary"; "join-item" ]
                        prop.text "+"
                        prop.onClick (Hooks.callback((), fun _ -> dispatch Incr))
                    ]
                    Html.button [
                        prop.classes [ "btn"; "btn-primary"; "join-item" ]
                        prop.text "-"
                        prop.onClick (Hooks.callback((), fun _ -> dispatch Decr))
                    ]
                    Html.button [
                        prop.classes [ "btn"; "btn-secondary"; "join-item" ]
                        prop.text "Reset"
                        prop.onClick (Hooks.callback((), fun _ -> dispatch Reset))
                    ]
                ]
            ]
        ]
    ]

let private homeView dispatch =
    Html.div [
        prop.classes [ "space-y-4" ]
        prop.children [
            Html.h1 [
                prop.classes [ "text-2xl"; "font-bold" ]
                prop.text "Diabetes diary"
            ]
            Html.p [
                prop.classes [ "max-w-prose" ]
                prop.text "Your diary dashboard will live here."
            ]
            Html.button [
                prop.classes [ "btn"; "btn-primary" ]
                prop.text "Open counter"
                prop.onClick (Hooks.callback((), fun _ -> dispatch (Navigate Counter)))
            ]
        ]
    ]

let private notFoundView dispatch parts =
    let path = pathFromRoute (NotFound parts)

    Html.div [
        prop.classes [ "space-y-4" ]
        prop.children [
            Html.h1 [
                prop.classes [ "text-2xl"; "font-bold" ]
                prop.text "Page not found"
            ]
            Html.p [
                prop.classes [ "max-w-prose" ]
                prop.text $"No route matches {path}."
            ]
            Html.button [
                prop.classes [ "btn"; "btn-primary" ]
                prop.text "Go home"
                prop.onClick (Hooks.callback((), fun _ -> dispatch (Navigate Home)))
            ]
        ]
    ]

let private pageView dispatch model =
    match model.Route with
    | Home -> homeView dispatch
    | Counter -> counterView dispatch model
    | NotFound path -> notFoundView dispatch path

let view dispatch =

    fun model ->
    Html.div [
        prop.classes [ "min-h-screen"; "p-6"; "space-y-6" ]
        prop.children [
            Html.nav [
                prop.classes [ "flex"; "gap-2" ]
                prop.children [
                    navLink dispatch model.Route Home "Home"
                    navLink dispatch model.Route Counter "Counter"
                ]
            ]
            pageView dispatch model
        ]
    ]
