module Router

open System
open Browser.Dom
open Browser.Types
open Fable.Core
open Fastoch.Elmish

[<RequireQualifiedAccess>]
type HistoryMode =
    | PushState
    | ReplaceState

[<RequireQualifiedAccess>]
type RouteMode =
    | Path
    | Hash

[<Emit("encodeURIComponent($0)")>]
let private encodeURIComponent (value: string) : string = jsNative

[<Emit("decodeURIComponent($0)")>]
let private decodeURIComponent (value: string) : string = jsNative

let private normalizePath routeMode (path: string) =
    let path = if String.IsNullOrWhiteSpace path then "/" else path

    match routeMode with
    | RouteMode.Path ->
        if path.StartsWith "/" then path else "/" + path
    | RouteMode.Hash ->
        if path.StartsWith "#/" then path
        elif path.StartsWith "#" then "#/" + path.TrimStart('#', '/')
        elif path.StartsWith "/" then "#" + path
        else "#/" + path

let private encodePart (part: string) =
    if part.StartsWith "?" || part.StartsWith "#" || part.StartsWith "/" then
        part
    else
        encodeURIComponent part

let private decodePart (part: string) =
    decodeURIComponent part

let private decodeQueryPart (part: string) =
    part.Replace("+", " ") |> decodeURIComponent

let private currentPathString routeMode =
    match routeMode with
    | RouteMode.Path -> window.location.pathname + window.location.search
    | RouteMode.Hash -> window.location.hash

let private parseSegment (segment: string) =
    if String.IsNullOrWhiteSpace segment then
        []
    else
        let segment = segment.TrimEnd '#'
        let queryIndex = segment.IndexOf '?'

        if queryIndex = 0 then
            [ segment ]
        elif queryIndex > 0 then
            let pathPart = segment.Substring(0, queryIndex)
            let queryPart = segment.Substring queryIndex
            [ decodePart pathPart; queryPart ]
        else
            [ decodePart segment ]

let parsePath (path: string) =
    let path =
        if String.IsNullOrWhiteSpace path then
            ""
        elif path.StartsWith "#" then
            path.Substring 1
        else
            path

    path.Trim('/')
        .Split('/', StringSplitOptions.RemoveEmptyEntries)
    |> Array.toList
    |> List.collect parseSegment

let formatPartsWithMode routeMode parts =
    parts
    |> List.map encodePart
    |> String.concat "/"
    |> normalizePath routeMode

let formatParts parts =
    formatPartsWithMode RouteMode.Path parts

let formatHashParts parts =
    formatPartsWithMode RouteMode.Hash parts

let encodeQueryString query =
    query
    |> List.map (fun (key, value) ->
        String.concat "=" [ encodeURIComponent key; encodeURIComponent value ])
    |> String.concat "&"
    |> function
        | "" -> ""
        | value -> "?" + value

let parseQueryString (query: string) =
    let query = query.TrimStart '?'

    query.Split('&', StringSplitOptions.RemoveEmptyEntries)
    |> Array.toList
    |> List.map (fun entry ->
        let separatorIndex = entry.IndexOf '='

        if separatorIndex < 0 then
            decodeQueryPart entry, ""
        else
            let key = entry.Substring(0, separatorIndex)
            let value = entry.Substring(separatorIndex + 1)
            decodeQueryPart key, decodeQueryPart value)

let formatPartsWithQuery routeMode parts query =
    formatPartsWithMode routeMode parts + encodeQueryString query

let formatPathWithQuery parts query =
    formatPartsWithQuery RouteMode.Path parts query

let formatHashWithQuery parts query =
    formatPartsWithQuery RouteMode.Hash parts query

let current () =
    RouteMode.Path |> currentPathString |> parsePath

let currentWithMode routeMode =
    routeMode |> currentPathString |> parsePath

let navigateWithMode routeMode historyMode toMsg path : Cmd<'msg> =
    [ fun dispatch ->
        let path = normalizePath routeMode path

        if currentPathString routeMode <> path then
            match historyMode with
            | HistoryMode.PushState -> window.history.pushState(null, "", path)
            | HistoryMode.ReplaceState -> window.history.replaceState(null, "", path)

        routeMode |> currentWithMode |> toMsg |> dispatch
    ]

let navigate toMsg path =
    navigateWithMode RouteMode.Path HistoryMode.PushState toMsg path

let replace toMsg path =
    navigateWithMode RouteMode.Path HistoryMode.ReplaceState toMsg path

let navigateParts toMsg parts =
    parts |> formatParts |> navigate toMsg

let replaceParts toMsg parts =
    parts |> formatParts |> replace toMsg

let subscribeWithMode routeMode toMsg : Sub<'msg> =
    [ [ "router"; string routeMode ],
      fun dispatch ->
          let handler (_: Event) =
              routeMode |> currentWithMode |> toMsg |> dispatch

          match routeMode with
          | RouteMode.Path ->
              window.addEventListener("popstate", handler)
          | RouteMode.Hash ->
              window.addEventListener("hashchange", handler)
              window.addEventListener("popstate", handler)

          { new IDisposable with
              member _.Dispose() =
                  match routeMode with
                  | RouteMode.Path ->
                      window.removeEventListener("popstate", handler)
                  | RouteMode.Hash ->
                      window.removeEventListener("hashchange", handler)
                      window.removeEventListener("popstate", handler) } ]

let subscribe toMsg =
    subscribeWithMode RouteMode.Path toMsg

module Route =
    let (|Int|_|) (input: string) =
        match Int32.TryParse input with
        | true, value -> Some value
        | _ -> None

    let (|Int64|_|) (input: string) =
        match Int64.TryParse input with
        | true, value -> Some value
        | _ -> None

    let (|Guid|_|) (input: string) =
        match Guid.TryParse input with
        | true, value -> Some value
        | _ -> None

    let (|Number|_|) (input: string) =
        match Double.TryParse input with
        | true, value -> Some value
        | _ -> None

    let (|Decimal|_|) (input: string) =
        match Decimal.TryParse input with
        | true, value -> Some value
        | _ -> None

    let (|Bool|_|) (input: string) =
        match input.ToLowerInvariant() with
        | "1" | "true" | "" -> Some true
        | "0" | "false" -> Some false
        | _ -> None

    let (|Query|_|) (input: string) =
        if input.StartsWith "?" then
            input |> parseQueryString |> Some
        else
            None
