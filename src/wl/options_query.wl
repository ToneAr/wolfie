Function[
	{head},
	Quiet[
		Check[
			StringRiffle[
				ToString /@ First /@ Options[ToExpression[head]],
				"\n"
			],
			""
		]
	]
]