Function[
	{source, scriptCommandLine, evaluationEnvironment, inputFileName},
	Module[{stream, held, result = Null},
		stream = StringToStream[source];
		Internal`WithLocalSettings[
			Null,
			Block[{
					$ScriptCommandLine = scriptCommandLine,
					$EvaluationEnvironment =
						If[StringQ[evaluationEnvironment],
							evaluationEnvironment,
							$EvaluationEnvironment
						],
					System`Private`$InputFileName = inputFileName
				},
				While[
					True,
					held = Read[stream, HoldComplete[Expression]];
					If[held === EndOfFile, Break[]];
					result = ReleaseHold[held]
				];
				result
			],
			Close[stream]
		]
	]
]