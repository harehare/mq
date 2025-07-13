# Builtin selectors

| Selector Name           | Description                                                     | Parameters      | Example                 |
| ----------------------- | --------------------------------------------------------------- | --------------- | ----------------------- |
| `.h`, `.h(depth)`       | Selects a heading node with the specified depth.                | None, `depth`   | `.h`, `.h(6)`           |
| `.h1`                   | Selects a heading node with the 1 depth.                        | None            | `.h1`                   |
| `.h2`                   | Selects a heading node with the 2 depth.                        | None            | `.h2`                   |
| `.h3`                   | Selects a heading node with the 3 depth.                        | None            | `.h3`                   |
| `.h4`                   | Selects a heading node with the 4 depth.                        | None            | `.h4`                   |
| `.h5`                   | Selects a heading node with the 5 depth.                        | None            | `.h5`                   |
| `.h6`                   | Selects a heading node with the 6 depth.                        | None            | `.h6`                   |
| `.code`                 | Selects a code block node with the specified language.          | `lang`          | `.code "rust"`          |
| `.code_inline`          | Selects an inline code node.                                    | None            | `.code_inline`          |
| `.inline_math`          | Selects an inline math node.                                    | None            | `.inline_math`          |
| `.strong`               | Selects a strong (bold) node.                                   | None            | `.strong`               |
| `.emphasis`             | Selects an emphasis (italic) node.                              | None            | `.emphasis`             |
| `.delete`               | Selects a delete (strikethrough) node.                          | None            | `.delete`               |
| `.link`                 | Selects a link node.                                            | None            | `.link`                 |
| `.link_ref`             | Selects a link reference node.                                  | None            | `.link_ref`             |
| `.image`                | Selects an image node.                                          | None            | `.image`                |
| `.heading`              | Selects a heading node with the specified depth.                | None            | `.heading 1`            |
| `.horizontal_rule`      | Selects a horizontal rule node.                                 | None            | `.horizontal_rule`      |
| `.blockquote`           | Selects a blockquote node.                                      | None            | `.blockquote`           |
| `.[][]`                 | Selects a table cell node with the specified row and column.    | `row`, `column` | `.[1][1]`               |
| `.html` ,`.<>`          | Selects an HTML node.                                           | None            | `.html`, `.<>`          |
| `.footnote`             | Selects a footnote node.                                        | None            | `.footnote`             |
| `.mdx_jsx_flow_element` | Selects an MDX JSX flow element node.                           | None            | `.mdx_jsx_flow_element` |
| `.list`,`.[]`           | Selects a list node with the specified index and checked state. | `indent`        | `.list(1)`, `.[1]`      |
| `.mdx_js_esm`           | Selects an MDX JS ESM node.                                     | None            | `.mdx_js_esm`           |
| `.toml`                 | Selects a TOML node.                                            | None            | `.toml`                 |
| `.text `                | Selects a Text node.                                            | None            | `.text`                 |
| `.yaml`                 | Selects a YAML node.                                            | None            | `.yaml`                 |
| `.break`                | Selects a break node.                                           | None            | `.break`                |
| `.mdx_text_expression`  | Selects an MDX text expression node.                            | None            | `.mdx_text_expression`  |
| `.footnote_ref`         | Selects a footnote reference node.                              | None            | `.footnote_ref`         |
| `.image_ref`            | Selects an image reference node.                                | None            | `.image_ref`            |
| `.mdx_jsx_text_element` | Selects an MDX JSX text element node.                           | None            | `.mdx_jsx_text_element` |
| `.math`                 | Selects a math node.                                            | None            | `.math`                 |
| `.math_inline`          | Selects a math inline node.                                     | None            | `.math_inline`          |
| `.mdx_flow_expression`  | Selects an MDX flow expression node.                            | None            | `.mdx_flow_expression`  |
| `.definition`           | Selects a definition node.                                      | None            | `.definition`           |
