if exists("b:current_syntax")
  finish
endif

syntax keyword mqKeyword def let if elif else while foreach until self nodes fn
syntax keyword mqOperator -> = | : ; ?
syntax keyword mqBoolean true false
syntax keyword mqConstant None

syntax match mqComment "#.*$"

syntax region mqString start=/"/ end=/"/ contains=@Spell
syntax region mqInterpolatedString start=/s"/ end=/"/ contains=mqInterpolation,@Spell
syntax match mqInterpolation /\${[^}]*}/ contained containedin=mqInterpolatedString

syntax match mqNumber "\<\d\+\>"

syntax match mqFunctionCall "\<\w\+\s*("me=e-1 contains=mqFunctionCallArgs
syntax region mqFunctionCallArgs matchgroup=mqDelimiter start="(" end=")" contained transparent

syntax match mqFunctionDefine "\<def\s\+\w\+\s*("me=e-1
syntax match mqSelector "\.\w\+"
syntax match mqSelector "\.\["
syntax match mqSelector "\]"

highlight default link mqKeyword Keyword
highlight default link mqOperator Operator
highlight default link mqBoolean Boolean
highlight default link mqConstant Constant
highlight default link mqComment Comment
highlight default link mqString String
highlight default link mqInterpolatedString String
highlight default link mqInterpolation Interpolation
highlight default link mqNumber Number
highlight default link mqFunctionCall Function
highlight default link mqFunctionDefine Function
highlight default link mqSelector Statement

let b:current_syntax = "mq"
