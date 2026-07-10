Function[
	{input},
	Internal`WithLocalSettings[
		Off[General::shdw],
		ReleaseHold[ToExpression[input, InputForm, HoldComplete]],
		On[General::shdw]
	]
]