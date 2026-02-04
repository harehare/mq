; Keywords
[
  "def"
  "let"
  "if"
  "elif"
  "else"
  "end"
  "while"
  "foreach"
  "include"
  "fn"
  "do"
  "var"
  "macro"
] @keyword

; Statement keywords
(break_expr) @keyword
(continue_expr) @keyword

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
  "+="
  "-="
  "%="
  "/="
  "//="
  "|="
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
(def_expr
  name: (identifier) @function)

; Function calls
(call_expr
  function: (identifier) @function.call)

; Parameters
(parameter
  name: (identifier) @parameter)

; Variables
(let_expr
  name: (identifier) @variable)

(foreach_expr
  variable: (identifier) @variable)

; Identifiers
(identifier) @variable

; Comments
(comment) @comment
