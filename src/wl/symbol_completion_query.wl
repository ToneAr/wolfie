With[{p = __PREFIX__},
  Module[{
      contexts = Contexts[],
      matchingContexts,
      rawSymbols,
      symbols,
      contextOf,
      shortName,
      isPrivateContext,
      item
    },
    contextOf[name_] :=
      If[StringContainsQ[name, "`"],
        StringReplace[name, RegularExpression["^(.*`).*$"] -> "$1"],
        ""
      ];
    shortName[name_] :=
      If[StringContainsQ[name, "`"],
        StringReplace[name, RegularExpression["^.*`"] -> ""],
        name
      ];
    isPrivateContext[context_] :=
      context === "Private`" || StringEndsQ[context, "`Private`"];
    matchingContexts =
      Select[contexts, StringStartsQ[#, p] && !isPrivateContext[#]&];
    rawSymbols =
      If[StringContainsQ[p, "`"],
        Names[StringJoin[ p, "*"]],
        Names[StringJoin["*`", p, "*"]]
   ];
    symbols = Select[rawSymbols, !isPrivateContext[contextOf[#]]&];
    item[name_] :=
      StringRiffle[{"symbol", shortName[name], "0", contextOf[name]}, "\t"];
    StringRiffle[
      Take[
        DeleteDuplicates[
          Join[
            (StringJoin[ "context\t", #, "\t0\t", #])& /@ matchingContexts,
            item /@ symbols
          ]
        ],
        UpTo[500]
      ],
      "\n"
    ]
  ]
]
