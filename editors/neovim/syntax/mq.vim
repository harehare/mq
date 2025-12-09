if exists("b:current_syntax")
  finish
endif

" Comments (highest priority to avoid conflicts)
syn match mqComment "#.*$" contains=mqTodo
syn keyword mqTodo contained TODO FIXME XXX NOTE

" Keywords (use \< and \> for word boundaries to avoid partial matches)
syn match mqKeywordControl "\<\(def\|do\|if\|elif\|else\|end\|while\|foreach\|fn\|break\|continue\|match\|var\)\>"
syn match mqKeywordInclude "\<\(include\|module\|import\)\>"
syn match mqKeywordSpecial "\<\(self\|nodes\)\>"
syn match mqKeywordLet "\<let\>" nextgroup=mqVariableDef skipwhite

" Boolean and constants (use \< and \> for word boundaries)
syn match mqBoolean "\<\(true\|false\)\>"
syn match mqConstant "\<None\>"

" Operators
syn match mqOperator "|"
syn match mqOperator ":"
syn match mqOperator ";"
syn match mqOperator "?"
syn match mqOperator "!"
syn match mqOperator "+"
syn match mqOperator "-"
syn match mqOperator "\*"
syn match mqOperator "/"
syn match mqOperator "%"
syn match mqOperator "<="
syn match mqOperator ">="
syn match mqOperator "=="
syn match mqOperator "!="
syn match mqOperator "&&"
syn match mqOperator "||"

" Numbers (including floats)
syn match mqNumber "\v<\d+>"
syn match mqNumber "\v<\d+\.\d+>"
syn match mqNumber "\v<0x[0-9a-fA-F]+>"
syn match mqNumber "\v<0b[01]+>"

" Strings
syn region mqString start='"' end='"' skip='\\"' contains=mqEscape
syn match mqEscape "\\." contained

" Interpolated strings
syn region mqStringInterpolate start='s"' end='"' skip='\\"' contains=mqInterpolation,mqEscape
syn region mqInterpolation matchgroup=mqInterpolationDelimiter start="\${" end="}" contained contains=mqVariableRef,mqFunctionCall,mqSelector,mqOperator,mqNumber

" Variable references in interpolation
syn match mqVariableRef "\<[a-zA-Z_][a-zA-Z0-9_]*\>" contained

" Symbols
syn match mqSymbol ":\<[a-zA-Z_][a-zA-Z0-9_]*\>"

" Function definitions - must come before function calls
syn match mqFunctionDef "\<def\>\s\+\zs[a-zA-Z_][a-zA-Z0-9_]*\ze\s*(" contains=mqKeywordControl

" Variable definitions
syn match mqVariableDef "\<[a-zA-Z_][a-zA-Z0-9_]*\>" contained

" Selectors (must come before function calls to take precedence)
syn match mqSelector "\.\<[a-zA-Z_][a-zA-Z0-9_]*\>"
syn match mqSelector "\.\[\]"
syn match mqSelector "\.\[\]\[\]"
syn match mqSelector "\.h\d\@!"
syn match mqSelector "\.h[1-6]\>"

" Function calls (must come after selectors and function definitions)
syn match mqFunctionCall "\<[a-zA-Z_][a-zA-Z0-9_]*\>\ze\s*("

" Delimiters
syn match mqDelimiter "("
syn match mqDelimiter ")"
syn match mqDelimiter "\["
syn match mqDelimiter "\]"
syn match mqDelimiter "{"
syn match mqDelimiter "}"
syn match mqDelimiter ","

" Highlighting groups
hi def link mqComment Comment
hi def link mqTodo Todo

hi def link mqKeywordControl Keyword
hi def link mqKeywordInclude Include
hi def link mqKeywordSpecial Special
hi def link mqKeywordLet Keyword

hi def link mqBoolean Boolean
hi def link mqConstant Constant
hi def link mqType Type

hi def link mqOperator Operator

hi def link mqNumber Number

hi def link mqString String
hi def link mqStringInterpolate String
hi def link mqEscape SpecialChar
hi def link mqInterpolationDelimiter Delimiter
hi def link mqVariableRef Identifier

hi def link mqSymbol Constant

hi def link mqFunctionDef Function
hi def link mqVariableDef Identifier

hi def link mqSelector Special
hi def link mqFunctionCall Function

hi def link mqDelimiter Delimiter

let b:current_syntax = "mq"
