Function[
	{p},
	Module[{
			contexts = Contexts[],
			currentContext = $Context,
			matchingContexts,
			currentContextSymbols,
			visibleSymbols,
			rawSymbols,
			contextOf,
			shortName,
			isPrivateContext,
			showsPrivateContext,
			isVisibleContext,
			item,
			items
		},
		contextOf =
			If[StringContainsQ[#1, "`"],
				StringReplace[#1, RegularExpression["^(.*`).*$"] -> "$1"],
				#2
			]&;
		shortName =
			If[StringContainsQ[#, "`"],
				StringReplace[#, RegularExpression["^.*`"] -> ""],
				#
			]&;
		isPrivateContext = (# === "Private`" || StringEndsQ[#, "`Private`"])&;
		showsPrivateContext = (isPrivateContext[#] && StringStartsQ[p, #])&;
		isVisibleContext = (!isPrivateContext[#] || showsPrivateContext[#])&;
		matchingContexts =
			Select[contexts, StringStartsQ[#, p] && !isPrivateContext[#]&];
		currentContextSymbols =
			If[StringContainsQ[p, "`"],
				{},
				Names[StringJoin[ currentContext, p, "*"]]
			];
		visibleSymbols =
			If[StringContainsQ[p, "`"], {}, Names[StringJoin[ p, "*"]]];
		rawSymbols =
			If[StringContainsQ[p, "`"],
				Names[StringJoin[ p, "*"]],
				Names[StringJoin[ "*`", p, "*"]]
			];
		item =
			StringRiffle[
				{"symbol", shortName[#1], "0", contextOf[#1, #2]},
				"\t"
			]&;
		items =
			Join[
				(StringJoin[ "context\t", #, "\t0\t", #])& /@ matchingContexts,
				item[#, currentContext]& /@ Select[
					currentContextSymbols,
					isVisibleContext[contextOf[#, currentContext]]&
				],
				item[#, ""]& /@ Select[
					visibleSymbols,
					isVisibleContext[contextOf[#, ""]]&
				],
				item[#, ""]& /@ Select[
					rawSymbols,
					isVisibleContext[contextOf[#, ""]]&
				]
			];
		StringRiffle[Take[DeleteDuplicates[items], UpTo[500]], "\n"]
	]
]