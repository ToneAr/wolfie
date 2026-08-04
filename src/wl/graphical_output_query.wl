Function[
	{graphic},
	If[
		FreeQ[
			ToBoxes[graphic],
			_GraphicsBox | _Graphics3DBox | _RasterBox,
			{0, Infinity}
		],
		"",
		With[{svg = ExportString[graphic, "SVG"]}, If[StringQ[svg], svg, ""]]
	]
]