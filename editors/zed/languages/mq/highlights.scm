; Keywords
[
  "def"
  "let"
  "if"
  "elif"
  "else"
  "end"
  "while"
  "loop"
  "foreach"
  "include"
  "import"
  "module"
  "match"
  "fn"
  "do"
  "var"
  "macro"
] @keyword

; Statement keywords
(break_expr) @keyword
(continue_expr) @keyword

; Special identifiers
(self) @variable.builtin
(nodes) @variable.builtin

; Literals
(boolean) @boolean
(none) @constant.builtin
(number) @number
(string) @string
(symbol) @string.special.symbol

; Interpolated strings
(interpolated_string) @string
(string_content) @string
(escaped_dollar) @string.escape

; Interpolation
(interpolation
  "${" @punctuation.special
  "}" @punctuation.special) @embedded

; Type patterns
(type_pattern) @type

; Wildcard pattern
(wildcard_pattern) @variable.builtin

; Rest pattern
(rest_pattern
  variable: (identifier) @variable)

; Operators
[
  "+"
  "-"
  "*"
  "/"
  "%"
  "="
  "=="
  "!="
  "+="
  "-="
  "*="
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
  "::"
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

; Module definitions
(module_expr
  name: (identifier) @module)

; Import paths
(import_expr
  path: (string) @string)

; Function definitions
(def_expr
  name: (identifier) @function)

; Macro definitions
(macro_expr
  name: (identifier) @function.macro)

; Function calls
(call_expr
  function: (identifier) @function.call)

; Qualified access (module::function)
(qualified_access
  module: (identifier) @module
  function: (identifier) @function.call)

; Parameters
(parameter
  name: (identifier) @variable.parameter)

; Variadic parameters
(variadic_parameter
  "*" @operator
  name: (identifier) @variable.parameter)

; Dict entry keys
(dict_entry
  key: (identifier) @property)

; Variables
(let_expr
  name: (identifier) @variable)

(var_expr
  name: (identifier) @variable)

(foreach_expr
  variable: (identifier) @variable)

; Variable patterns in match
(variable_pattern
  (identifier) @variable)

; Dict pattern keys
(dict_pattern
  key: (identifier) @property)

; Identifiers
(identifier) @variable

; Comments
(comment) @comment
