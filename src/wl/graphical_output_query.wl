Function[
	{graphic},
	If[
		FreeQ[
			ToBoxes[graphic],
			_GraphicsBox | _Graphics3DBox | _RasterBox,
			{0, Infinity}
		],
		"",
		With[
			{
				background = Replace[
					Options[graphic, Background],
					{
						{Background -> value_} :> value,
						{} -> None
					}
				]
			},
			With[
				{svg = ExportString[graphic, "SVG", Background -> background]},
				If[StringQ[svg], svg, ""]
			]
		]
	]
]
