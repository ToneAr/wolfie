Function[
	{input},
	Internal`WithLocalSettings[
		Off[General::shdw],
		ToExpression[input],
		On[General::shdw]
	]
]