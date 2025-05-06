" Vim syntax file
" Language: mq query language
" Maintainer: mq maintainers
" Latest Revision: 2025-05-06

if exists("b:current_syntax")
  finish
endif

" Keywords
syntax keyword mqKeyword select where map filter
syntax keyword mqOperator and or not
syntax keyword mqFunction contains startsWith endsWith length toUpperCase toLowerCase
syntax keyword mqSelector Heading1 Heading2 Heading3 Heading4 Heading5 List CheckedList Table Code InlineCode Math InlineMath Html Yaml Toml
syntax keyword mqBoolean true false

" Strings
syntax region mqString start=/"/ skip=/\\"/ end=/"/ oneline
syntax region mqString start=/'/ skip=/\\'/ end=/'/ oneline

" Numbers
syntax match mqNumber /\<\d\+\>/
syntax match mqFloat /\<\d\+\.\d*\>/

" Comments
syntax match mqComment /#.*$/

" Function calls and operations
syntax match mqFunctionCall /\<\w\+\ze(/
syntax match mqDotNotation /\.\w\+/

" Regular expressions
syntax region mqRegex start=// skip=/\\\// end=// oneline

" Parentheses and brackets
syntax region mqBlock start=/{/ end=/}/ transparent fold
syntax region mqList start=/\[/ end=/\]/ transparent fold
syntax region mqParens start=/(/ end=/)/ transparent fold

" Set highlighting
highlight default link mqKeyword Keyword
highlight default link mqOperator Operator
highlight default link mqFunction Function
highlight default link mqSelector Type
highlight default link mqString String
highlight default link mqNumber Number
highlight default link mqFloat Number
highlight default link mqComment Comment
highlight default link mqFunctionCall Function
highlight default link mqDotNotation Function
highlight default link mqRegex String
highlight default link mqBoolean Boolean

let b:current_syntax = "mq"
