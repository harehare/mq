; Keywords
[
  "def"
  "let"
  "if"
  "elif"
  "else"
  "end"
  "while"
  "until"
  "foreach"
  "include"
  "fn"
  "do"
] @keyword

; Statement keywords
(break_statement) @keyword
(continue_statement) @keyword

; Special identifiers
(self) @keyword.special
(nodes) @keyword.special

; Literals
(boolean) @boolean
(none) @constant.builtin
(number) @number
(string) @string

; Interpolation
(interpolation) @embedded

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "&&"
  "||"
  "!"
  ".."
  "|"
] @operator

; Punctuation
[
  ":"
  ";"
  ","
  "."
] @punctuation.delimiter

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

; Function definitions
(def_statement
  name: (identifier) @function)

; Function calls
(call_expression
  function: (identifier) @function.call)

; Parameters
(parameter_list
  (identifier) @parameter)

; Variables
(let_statement
  name: (identifier) @variable)

(foreach_statement
  variable: (identifier) @variable)

; Identifiers
(identifier) @variable

; Comments
(comment) @comment
